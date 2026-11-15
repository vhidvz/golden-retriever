# Sound sources

Every recording here is of a real dog and in the public domain (CC0), from OpenGameArt, Wikimedia Commons and Freesound.
The "80 CC0 creature SFX" pack was considered and left out: its sounds are voiced by a person.

| Files | Source | Author | Licence |
|---|---|---|---|
| `bark_1.ogg` … `bark_4.ogg` (four barks cut from one recording) | https://opengameart.org/content/dog-barking-mono | Brandon Morris | CC0 |
| `sigh_1.ogg`, `dream_1.ogg` (the same short grunt) | https://opengameart.org/content/dog-grunt | qubodup | CC0 |
| `whine_1.ogg` … `whine_3.ogg` (three whines cut from "Sad Dog.wav") | https://opengameart.org/content/dog-sounds | qubodup | CC0 |
| `whine_pup_1.ogg` … `whine_pup_4.ogg` (four whimpers of a puppy at night, cut, band-filtered and denoised) | https://commons.wikimedia.org/wiki/File:Puppy_at_a_night.ogg ("A crying normal puppy") | Knites | CC0 |
| `yawn_1.ogg` (1.1 s from 0.0 s of "Dog Yawn") | https://freesound.org/people/qubodup/sounds/827660/ | qubodup | CC0 |
| `yawn_2.ogg` (2.04 s from 0.0 s of "Dog Yawning") | https://freesound.org/people/qubodup/sounds/826022/ | qubodup | CC0 |
| `yawn_3.ogg` (1.77 s from 0.0 s of "fangsie yawn.wav") | https://freesound.org/people/jinxycat49/sounds/490164/ | jinxycat49 | CC0 |
| `shake_1.ogg` (1.33 s from 0.0 s of "Dog shaking himself dry") | https://freesound.org/people/giddster/sounds/435368/ | giddster | CC0 |
| `shake_2.ogg` (2.41 s from 0.0 s of "Dog shaking it off") | https://freesound.org/people/PicklesSuck/sounds/547569/ | PicklesSuck | CC0 |
| `sniff_1.ogg` (1.12 s from 0.0 s of "DogSniff01.wav") | https://freesound.org/people/nickyg11p/sounds/390850/ | nickyg11p | CC0 |
| `sniff_2.ogg` (1.73 s from 0.0 s of "Dog sniff 2") | https://freesound.org/people/fthgurdy/sounds/528938/ | fthgurdy | CC0 |
| `drink_1.ogg` (2.6 s from 0.5 s of "Dog Drinking Water 1_1") | https://freesound.org/people/Joao_Janz/sounds/483696/ | Joao_Janz | CC0 |
| `drink_2.ogg` (2.6 s from 3.55 s of "Dog Drinking - Water") | https://freesound.org/people/AudioWay/sounds/547339/ | AudioWay | CC0 |
| `drink_3.ogg` (2.6 s from 3.15 s of "Dog drink .wav") | https://freesound.org/people/Pipppo89/sounds/665108/ | Pipppo89 | CC0 |
| `drink_pup_1.ogg` (2.6 s from 2.95 s of "Drinking Water.wav") | https://freesound.org/people/YUXUANZHAO/sounds/625277/ | YUXUANZHAO | CC0 |
| `eat_1.ogg` (2.6 s from 0.1 s of "Dog Crunching Crackers") | https://freesound.org/people/Bon_Vivant_Pictures/sounds/456366/ | Bon_Vivant_Pictures | CC0 |
| `eat_2.ogg` (2.4 s from 0.1 s of "Eating.wav") | https://freesound.org/people/D123x/sounds/412310/ | D123x | CC0 |
| `eat_3.ogg` (2.6 s from 0.65 s of "Dog eats") | https://freesound.org/people/16FVolejnikovaA/sounds/498396/ | 16FVolejnikovaA | CC0 |
| `eat_pup_1.ogg` (2.6 s from 0.0 s of "Small dog eating crackers"; a miniature poodle, standing in for a puppy) | https://freesound.org/people/mewpudding101/sounds/265637/ | mewpudding101 | CC0 |

The Freesound recordings were found with `tools/freesound_fetch.py` (Freesound's API, only
sounds marked CC0 and tagged "dog"), chosen by what their uploaders wrote about them,
and checked by measurement: each one kept stands at least 12 dB above its background
noise. Long recordings were cut to their busiest stretch; all were levelled to the same
loudness and given soft edges.

To add a recording, drop a file named `<kind>_<anything>.ogg` or `.wav` here or in
`~/.local/share/buddy/sounds/`.

Recordings named `<kind>_pup_<anything>` are used instead while he is young: of a real puppy
(the whimpers, the drinking) or of a small dog standing in for one (the eating).
