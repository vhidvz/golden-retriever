//! The model catalogue and the fit heuristic. The catalogue is data, not code: adding a
//! model means adding an entry and evaluating it, not changing the selection algorithm.

use crate::ai_hardware::Hardware;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Model {
    pub name: String,
    pub params: String,
    pub download_gb: f64,
    pub capabilities: Vec<String>,
    pub license: String,
    pub min_ram_gb: f64,
    pub min_vram_gb: f64,
    pub cpu_friendly: bool,
    pub quality: u8,
    pub verified: bool,
    #[serde(default)]
    pub note: String,
}

impl Model {
    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    pub fn is_cloud(&self) -> bool {
        self.name.contains(":cloud") || self.name.ends_with("-cloud")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Catalog {
    pub models: Vec<Model>,
}

impl Catalog {
    pub fn load() -> Result<Self> {
        let models: Vec<Model> =
            serde_json::from_str(include_str!("../data/ai_models.json")).context("the bundled model catalogue is not valid JSON")?;
        Ok(Self { models })
    }

    pub fn find(&self, name: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.name == name)
    }

    /// The best fit for `needs`, plus up to two alternatives. `None` when nothing fits.
    pub fn recommend<'a>(&'a self, hardware: &Hardware, needs: &[&str]) -> Option<Recommendation<'a>> {
        let gpu_fits = |model: &Model| model.min_vram_gb > 0.0 && hardware.gpu.as_ref().is_some_and(|gpu| gpu.vram_gb >= model.min_vram_gb);
        let mut fits: Vec<(f64, &Model)> = self
            .models
            .iter()
            .filter(|model| !model.is_cloud())
            .filter(|model| needs.iter().all(|need| model.has(need)))
            .filter(|model| hardware.ram_gb >= model.min_ram_gb)
            .filter(|model| {
                let available = if hardware.ram_available_gb > 0.0 { hardware.ram_available_gb } else { hardware.ram_gb };
                available + 2.0 >= model.min_ram_gb
            })
            .filter(|model| hardware.free_disk_gb <= 0.0 || hardware.free_disk_gb >= model.download_gb + 2.0)
            .filter(|model| model.cpu_friendly || gpu_fits(model))
            .map(|model| {
                let on_gpu = gpu_fits(model);
                let mut score = f64::from(model.quality) * 10.0 - model.download_gb * 0.1;
                if model.verified {
                    score += 3.0;
                }
                if on_gpu {
                    score += 2.0;
                } else if model.cpu_friendly {
                    score += 2.0;
                }
                (score, model)
            })
            .collect();
        fits.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        let (_, default) = *fits.first()?;
        Some(Recommendation { default, alternatives: fits.iter().skip(1).take(2).map(|(_, model)| *model).collect() })
    }
}

pub struct Recommendation<'a> {
    pub default: &'a Model,
    pub alternatives: Vec<&'a Model>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_hardware::Gpu;

    fn hardware(ram_gb: f64, vram_gb: Option<f64>) -> Hardware {
        Hardware {
            ram_gb,
            gpu: vram_gb.map(|vram_gb| Gpu { name: "test gpu".into(), vendor: "test".into(), vram_gb, driver: "test".into(), compute: None }),
            free_disk_gb: 100.0,
            ..Hardware::default()
        }
    }

    const NEEDS: [&str; 4] = ["text", "tools", "json", "bilingual"];

    #[test]
    fn the_catalogue_has_the_verified_default_and_no_cloud_models() {
        let catalog = Catalog::load().unwrap();
        let gemma = catalog.find("gemma4:e4b").unwrap();
        assert!(gemma.verified);
        assert!(gemma.has("tools") && gemma.has("json") && gemma.has("bilingual"));
        assert!(catalog.models.iter().all(|m| !m.is_cloud()), "cloud models are never offered");
    }

    #[test]
    fn a_machine_with_31_gb_and_a_small_gpu_gets_the_verified_cpu_model() {
        let catalog = Catalog::load().unwrap();
        let pick = catalog.recommend(&hardware(31.18, Some(4.0)), &NEEDS).unwrap();
        assert_eq!(pick.default.name, "gemma4:e4b");
        assert!(pick.alternatives.len() <= 2);
    }

    #[test]
    fn an_8_gb_machine_gets_a_smaller_model() {
        let catalog = Catalog::load().unwrap();
        let pick = catalog.recommend(&hardware(8.0, None), &NEEDS).unwrap();
        assert!(pick.default.min_ram_gb <= 8.0, "{} needs {} GB", pick.default.name, pick.default.min_ram_gb);
        assert!(pick.default.has("tools"));
        assert_ne!(pick.default.name, "gemma4:e4b");
    }

    #[test]
    fn a_large_gpu_can_choose_a_larger_model() {
        let catalog = Catalog::load().unwrap();
        let pick = catalog.recommend(&hardware(64.0, Some(24.0)), &NEEDS).unwrap();
        assert_eq!(pick.default.name, "gemma4:26b");
    }

    #[test]
    fn a_busy_machine_does_not_get_a_model_that_needs_all_its_memory() {
        let catalog = Catalog::load().unwrap();
        let mut machine = hardware(16.0, None);
        machine.ram_available_gb = 2.0;
        let pick = catalog.recommend(&machine, &NEEDS).unwrap();
        assert_ne!(pick.default.name, "gemma4:e4b");
        assert!(pick.default.min_ram_gb <= machine.ram_available_gb + 2.0, "{} does not fit", pick.default.name);
    }

    #[test]
    fn a_small_disk_excludes_a_large_model() {
        let catalog = Catalog::load().unwrap();
        let mut machine = hardware(64.0, Some(24.0));
        machine.free_disk_gb = 5.0;
        let pick = catalog.recommend(&machine, &NEEDS).unwrap();
        assert_ne!(pick.default.name, "gemma4:26b");
        assert!(pick.default.download_gb + 2.0 <= machine.free_disk_gb, "{} does not fit", pick.default.name);
    }

    #[test]
    fn a_required_capability_filters_the_list() {
        let catalog = Catalog::load().unwrap();
        let pick = catalog.recommend(&hardware(31.18, Some(4.0)), &["text", "tools", "json", "bilingual", "vision"]).unwrap();
        assert!(pick.default.has("vision"));
    }

    #[test]
    fn a_machine_that_cannot_fit_anything_gets_no_recommendation() {
        let catalog = Catalog::load().unwrap();
        assert!(catalog.recommend(&hardware(2.0, None), &NEEDS).is_none());
    }
}
