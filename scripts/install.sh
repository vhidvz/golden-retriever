#!/bin/sh
# Installs Buddy for the current user only (no root): the binary, the renderer
# project, the perception helpers and a systemd user service that starts him with the
# graphical session. It also installs a pinned Ollama runtime under Buddy's data
# directory, unless --no-ai is given.
#   scripts/install.sh            install and start
#   scripts/install.sh --no-ai    install Buddy without a local model runtime
#   scripts/install.sh --remove   stop and remove (his memory and models are kept)
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
bin="$HOME/.local/bin"
share="${BUDDY_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/buddy}"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
extension="buddy@buddy.local"
extensions="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions"

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

port_free() {
    if command -v ss >/dev/null 2>&1; then
        ! ss -ltn 2>/dev/null | awk 'NR > 1 { print $4 }' | grep -q ":$1$"
    elif command -v curl >/dev/null 2>&1; then
        # Any HTTP response means something is holding the port; a refused connection
        # means it is free.
        ! curl -sS --max-time 1 -o /dev/null "http://127.0.0.1:$1/" >/dev/null 2>&1
    else
        return 1
    fi
}

choose_port() {
    p=$1
    while [ "$p" -le "$2" ]; do
        if port_free "$p"; then
            printf '%s' "$p"
            return 0
        fi
        p=$((p + 1))
    done
    return 1
}

extract_archive() {
    case "$1" in
        *.tar.zst)
            zstd -dc "$1" | tar -xf - -C "$2"
            ;;
        *.tgz)
            tar -xzf "$1" -C "$2"
            ;;
    esac
}


write_buddy_env() {
    key="$1"
    value="$2"
    env_file="$HOME/.config/buddy/env"
    mkdir -p "$(dirname "$env_file")"
    touch "$env_file"
    tmp="$env_file.tmp.$$"
    grep -v "^${key}=" "$env_file" > "$tmp" 2>/dev/null || true
    printf '%s="%s"\n' "$key" "$value" >> "$tmp"
    mv "$tmp" "$env_file"
    chmod 600 "$env_file"
}

install_godot() {
    umask 077
    if [ -n "${BUDDY_GODOT:-}" ] && [ -x "$BUDDY_GODOT" ]; then
        write_buddy_env BUDDY_GODOT "$BUDDY_GODOT"
        say "Using Godot at $BUDDY_GODOT"
        return 0
    fi
    if command -v godot >/dev/null 2>&1; then
        found=$(command -v godot)
        write_buddy_env BUDDY_GODOT "$found"
        say "Using Godot at $found"
        return 0
    fi
    if command -v godot4 >/dev/null 2>&1; then
        found=$(command -v godot4)
        write_buddy_env BUDDY_GODOT "$found"
        say "Using Godot at $found"
        return 0
    fi
    case "$(uname -m)" in
        x86_64) arch=x86_64 ;;
        aarch64 | arm64) arch=arm64 ;;
        *)
            warn "unsupported architecture $(uname -m); Godot was not installed and Buddy will not be drawn"
            return 0
            ;;
    esac
    command -v curl >/dev/null 2>&1 || {
        warn "curl is missing; Godot was not installed and Buddy will not be drawn"
        return 0
    }
    command -v unzip >/dev/null 2>&1 || {
        warn "unzip is missing; Godot was not installed and Buddy will not be drawn"
        return 0
    }
    command -v sha512sum >/dev/null 2>&1 || {
        warn "sha512sum is missing; Godot was not installed and Buddy will not be drawn"
        return 0
    }

    version="${GODOT_VERSION:-4.7.2}"
    asset="Godot_v${version}-stable_linux.${arch}.zip"
    base="https://github.com/godotengine/godot/releases/download/${version}-stable"
    odir="$share/godot"
    downloads="$odir/downloads"
    dest="$odir/$version"
    godot_bin="$dest/Godot_v${version}-stable_linux.${arch}"
    archive="$downloads/$asset"
    if [ -x "$godot_bin" ]; then
        write_buddy_env BUDDY_GODOT "$godot_bin"
        say "Godot $version is already installed at $godot_bin"
        return 0
    fi
    mkdir -p "$downloads" "$dest"
    say "Downloading Godot $version ..."
    if ! curl -fL --retry 5 --retry-all-errors --retry-delay 2 -C - --progress-bar -o "$archive" "$base/$asset"; then
        warn "the Godot download failed; Buddy will live without being drawn"
        return 0
    fi
    if ! curl -fsSL --retry 3 -o "$downloads/SHA512-SUMS.txt" "$base/SHA512-SUMS.txt"; then
        warn "the Godot checksum list could not be downloaded"
        return 0
    fi
    expected=$(awk -v f="$asset" '{ name=$2; sub(/^\*/, "", name); sub(/^\.\//, "", name); if (name == f) print $1 }' "$downloads/SHA512-SUMS.txt" | head -1)
    if [ -z "$expected" ]; then
        warn "no checksum for $asset in SHA512-SUMS.txt"
        return 0
    fi
    actual=$(sha512sum "$archive" | awk '{ print $1 }')
    if [ "$actual" != "$expected" ]; then
        warn "the Godot download failed its checksum; Buddy will not be drawn"
        rm -f "$archive"
        return 0
    fi
    if ! unzip -q -o "$archive" -d "$dest"; then
        warn "the Godot archive could not be extracted; Buddy will not be drawn"
        return 0
    fi
    if [ ! -f "$godot_bin" ]; then
        warn "the Godot binary was not found after extraction; Buddy will not be drawn"
        return 0
    fi
    chmod +x "$godot_bin"
    if ! "$godot_bin" --version >/dev/null 2>&1; then
        warn "Godot was downloaded but does not run; Buddy will not be drawn"
        return 0
    fi
    write_buddy_env BUDDY_GODOT "$godot_bin"
    say "Godot $version is installed at $godot_bin"
}

install_ai_runtime() {
    umask 077
    command -v curl >/dev/null 2>&1 || {
        warn "curl is missing; the local model runtime was not installed"
        return 0
    }
    command -v sha256sum >/dev/null 2>&1 || {
        warn "sha256sum is missing; the local model runtime was not installed"
        return 0
    }
    case "$(uname -m)" in
        x86_64) arch=amd64 ;;
        aarch64 | arm64) arch=arm64 ;;
        *)
            warn "unsupported architecture $(uname -m); the local model runtime was not installed"
            return 0
            ;;
    esac

    version="${OLLAMA_VERSION:-0.40.17}"
    odir="$share/ollama"
    vdir="$odir/$version"
    models="$odir/models"
    downloads="$odir/downloads"
    # Ollama writes its identity key under $HOME/.ollama. Give it a private home inside
    # Buddy's directory so ProtectHome=read-only does not make the service crash-loop.
    runtime_home="$odir/home"
    mkdir -p "$models" "$downloads" "$runtime_home"

    free_kb=$(df -Pk "$share" 2>/dev/null | awk 'NR == 2 { print $4 }' || true)
    if [ -n "${free_kb:-}" ] && [ "$free_kb" -lt 4194304 ]; then
        warn "less than 4 GB free under $share; the local model runtime was not installed"
        return 0
    fi

    if [ ! -x "$vdir/bin/ollama" ]; then
        base="https://github.com/ollama/ollama/releases/download/v$version"
        if curl -fsS --head --location "$base/ollama-linux-$arch.tar.zst" >/dev/null 2>&1; then
            asset="ollama-linux-$arch.tar.zst"
            command -v zstd >/dev/null 2>&1 || {
                warn "zstd is missing; install it (e.g. pacman -S zstd) and run the installer again"
                return 0
            }
        else
            asset="ollama-linux-$arch.tgz"
        fi
        archive="$downloads/$asset"
        say "Downloading Ollama $version ($asset) ..."
        if ! curl -fL --retry 5 --retry-all-errors --retry-delay 2 -C - --progress-bar -o "$archive" "$base/$asset"; then
            warn "the Ollama download failed; Buddy will live without a local model"
            return 0
        fi
        if ! curl -fsSL --retry 3 -o "$downloads/sha256sum.txt" "$base/sha256sum.txt"; then
            warn "the Ollama checksum list could not be downloaded"
            return 0
        fi
        expected=$(awk -v f="$asset" '{ name=$2; sub(/^\*/, "", name); sub(/^\.\//, "", name); if (name == f) print $1 }' "$downloads/sha256sum.txt" | head -1)
        if [ -z "$expected" ]; then
            warn "no checksum for $asset in sha256sum.txt"
            return 0
        fi
        actual=$(sha256sum "$archive" | awk '{ print $1 }')
        if [ "$actual" != "$expected" ]; then
            warn "the Ollama download failed its checksum; the old runtime, if any, is unchanged"
            rm -f "$archive"
            return 0
        fi
        tmp="$odir/.tmp-$version-$$"
        rm -rf "$tmp"
        mkdir -p "$tmp"
        if ! extract_archive "$archive" "$tmp" || [ ! -x "$tmp/bin/ollama" ]; then
            warn "the Ollama archive could not be extracted"
            rm -rf "$tmp"
            return 0
        fi
        rm -rf "$vdir"
        mv "$tmp" "$vdir"
        say "Ollama $version is installed under $vdir"
    else
        say "Ollama $version is already installed under $vdir"
        expected=$(sed -n 's/.*"tarball_sha256":"\([^"]*\)".*/\1/p' "$odir/runtime.json" 2>/dev/null | head -1 || true)
    fi

    if [ "${OLLAMA_ROCM:-0}" = 1 ] && [ ! -f "$vdir/.rocm" ]; then
        rocm="ollama-linux-$arch-rocm.tar.zst"
        base="https://github.com/ollama/ollama/releases/download/v$version"
        say "Downloading the ROCm runtime payload ..."
        if curl -fL --retry 5 --retry-all-errors --retry-delay 2 -C - --progress-bar -o "$downloads/$rocm" "$base/$rocm" &&
            curl -fsSL --retry 3 -o "$downloads/sha256sum.txt" "$base/sha256sum.txt"; then
            rocm_sum=$(awk -v f="$rocm" '{ name=$2; sub(/^\*/, "", name); sub(/^\.\//, "", name); if (name == f) print $1 }' "$downloads/sha256sum.txt" | head -1)
            if [ -n "$rocm_sum" ] && [ "$(sha256sum "$downloads/$rocm" | awk '{ print $1 }')" = "$rocm_sum" ]; then
                zstd -dc "$downloads/$rocm" | tar -xf - -C "$vdir"
                touch "$vdir/.rocm"
            else
                warn "the ROCm payload failed its checksum; continuing with the base runtime"
            fi
        else
            warn "the ROCm payload could not be downloaded; continuing with the base runtime"
        fi
    fi

    port=""
    if [ -f "$odir/runtime.json" ]; then
        port=$(sed -n 's/.*"endpoint":"http:\/\/127\.0\.0\.1:\([0-9][0-9]*\)".*/\1/p' "$odir/runtime.json" | head -1 || true)
    fi
    if [ -n "$port" ] && ! port_free "$port"; then
        # Our own service may still hold the port from a previous install. Any other
        # occupant is left alone; pick a new port instead.
        if ! systemctl --user is-active buddy-ollama.service >/dev/null 2>&1; then
            port=""
        fi
    fi
    if [ -z "$port" ]; then
        port=$(choose_port 11435 11445 || true)
    fi
    if [ -z "$port" ]; then
        warn "no free loopback port for the local model runtime"
        return 0
    fi

    ln -sfn "$version" "$odir/current"
    sed -e "s|@OLLAMA_BIN@|$vdir/bin/ollama|g" \
        -e "s|@OLLAMA_HOST@|127.0.0.1:$port|g" \
        -e "s|@OLLAMA_MODELS@|$models|g" \
        -e "s|@OLLAMA_DIR@|$odir|g" \
        -e "s|@OLLAMA_HOME@|$runtime_home|g" \
        "$repo/dist/buddy-ollama.service.in" > "$units/buddy-ollama.service"

    if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
        systemctl --user daemon-reload
        systemctl --user enable buddy-ollama.service || warn "the Ollama user service could not be enabled"
        systemctl --user restart buddy-ollama.service || warn "the Ollama user service could not be started"
        i=0
        while [ "$i" -lt 30 ]; do
            if curl -fsS --max-time 1 "http://127.0.0.1:$port/api/version" >/dev/null 2>&1; then
                break
            fi
            i=$((i + 1))
            sleep 1
        done
        if curl -fsS --max-time 1 "http://127.0.0.1:$port/api/version" >/dev/null 2>&1; then
            say "The local model runtime is running on 127.0.0.1:$port"
        else
            warn "the runtime is installed but is not answering yet; check: systemctl --user status buddy-ollama.service"
        fi
    else
        warn "the systemd user manager is not available; the runtime is installed but not started"
    fi

    old_model=$(sed -n 's/.*"model":"\([^"]*\)".*/\1/p' "$odir/runtime.json" 2>/dev/null | head -1 || true)
    old_digest=$(sed -n 's/.*"model_digest":"\([^"]*\)".*/\1/p' "$odir/runtime.json" 2>/dev/null | head -1 || true)
    model_json=null
    digest_json=null
    [ -n "$old_model" ] && model_json="\"$(json_escape "$old_model")\""
    [ -n "$old_digest" ] && digest_json="\"$(json_escape "$old_digest")\""
    runtime_tmp="$odir/.runtime.json.$$"
    printf '{"schema":1,"ollama_version":"%s","tarball_sha256":"%s","endpoint":"http://127.0.0.1:%s","service":"buddy-ollama.service","models_dir":"%s","model":%s,"model_digest":%s,"installed_at":%s}\n' \
        "$(json_escape "$version")" "$(json_escape "${expected:-}")" "$port" "$(json_escape "$models")" \
        "$model_json" "$digest_json" "$(date +%s)" > "$runtime_tmp"
    mv "$runtime_tmp" "$odir/runtime.json"
}

remove_buddy() {
    systemctl --user disable --now buddy.service 2>/dev/null || true
    systemctl --user disable --now buddy-ollama.service 2>/dev/null || true
    rm -f "$units/buddy.service" "$units/buddy-ollama.service" "$units/buddy.service.d/data-dir.conf" "$bin/buddy"
    rmdir "$units/buddy.service.d" 2>/dev/null || true
    rm -rf "$share/renderer" "$share/perception" "$share/tools" "$share/godot" "$extensions/$extension"
    gnome-extensions disable "$extension" 2>/dev/null || true
    systemctl --user daemon-reload 2>/dev/null || true
    echo "Buddy removed. His memory, the managed AI runtime and its models are still in $share."
}

no_ai=0
case "${1:-}" in
    --remove)
        remove_buddy
        exit 0
        ;;
    --no-ai)
        no_ai=1
        ;;
    "")
        ;;
    *)
        echo "usage: scripts/install.sh [--no-ai|--remove]" >&2
        exit 2
        ;;
esac

command -v godot >/dev/null 2>&1 || command -v godot4 >/dev/null 2>&1 || [ -n "${BUDDY_GODOT:-}" ] ||
    echo "note: Godot was not found on PATH; the installer will use the managed copy under $share/godot or download Godot 4.7.2" >&2
command -v blender >/dev/null 2>&1 || [ -n "${BUDDY_BLENDER:-}" ] ||
    echo "note: Blender was not found; Buddy will be drawn live only, without the pictures with a real coat (install blender to have them)" >&2

(cd "$repo" && cargo build --release)
mkdir -p "$bin" "$share" "$units"
systemctl --user stop buddy.service 2>/dev/null || true
install -m 755 "$repo/target/release/buddy" "$bin/buddy"
rm -rf "$share/renderer"
mkdir "$share/renderer"
cp "$repo"/renderer/project.godot "$repo"/renderer/*.tscn "$repo"/renderer/*.gd "$repo"/renderer/*.gdshader "$repo"/renderer/*.gdshaderinc "$share/renderer/"
cp -r "$repo/renderer/sounds" "$share/renderer/sounds"
cp -r "$repo/renderer/dog" "$share/renderer/dog"
# The script that draws his pictures with a real coat, run by the daemon while you are away.
rm -rf "$share/tools"
mkdir "$share/tools"
cp "$repo/tools/bake_sprites.py" "$share/tools/"
rm -rf "$share/perception"
mkdir "$share/perception"
cp "$repo"/perception/*.py "$share/perception/"

# The renderer needs Godot; if it is not already available, install a pinned copy.
install_godot

# The desktop sense is a GNOME Shell extension. On Wayland a newly installed extension
# is only loaded at the next login, so it is switched on now and starts working then.
if command -v gnome-extensions >/dev/null 2>&1; then
    rm -rf "$extensions/$extension"
    mkdir -p "$extensions/$extension"
    cp "$repo/shell-extension/$extension"/* "$extensions/$extension/"
    if ! gnome-extensions enable "$extension" 2>/dev/null; then
        current=$(gsettings get org.gnome.shell enabled-extensions)
        case "$current" in
            *"$extension"*) ;;
            "@as []") gsettings set org.gnome.shell enabled-extensions "['$extension']" ;;
            *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$extension']" ;;
        esac
        echo "Buddy's desktop sense will start working after you next log in."
    fi
fi
install -m 644 "$repo/dist/buddy.service" "$units/buddy.service"
# If the caller chose a non-default data directory, the service must use the same one;
# otherwise the daemon and the CLI would look in different places.
if [ -n "${BUDDY_DATA_DIR:-}" ]; then
    mkdir -p "$units/buddy.service.d"
    printf '[Service]\nEnvironment="BUDDY_DATA_DIR=%s"\n' "$share" > "$units/buddy.service.d/data-dir.conf"
else
    rm -f "$units/buddy.service.d/data-dir.conf"
fi
systemctl --user daemon-reload
systemctl --user enable --now buddy.service

if [ "$no_ai" -eq 0 ]; then
    install_ai_runtime
fi

echo "Buddy is running and will start with your desktop session. Try: buddy status"
if [ "$no_ai" -eq 0 ]; then
    if command -v loginctl >/dev/null 2>&1; then
        if ! loginctl show-user "${USER:-$(id -un)}" 2>/dev/null | grep -q 'Linger=yes'; then
            echo "To start the local model runtime before login, run: loginctl enable-linger ${USER:-$(id -un)}"
        fi
    fi
    echo "Next: buddy ai setup   # choose and install a model for this machine"
    echo "Then: buddy ai status"
fi

# A plain interactive bash does not always have ~/.local/bin on its PATH; say so rather than
# leave the user with "command not found".
if ! env -i HOME="$HOME" USER="${USER:-}" TERM=dumb bash -ic 'command -v buddy' >/dev/null 2>&1; then
    echo
    echo "Note: in a new bash terminal the 'buddy' command will not be found, because $bin is not on bash's PATH."
    echo "To fix that once and for all, run:"
    echo "    echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc && source ~/.bashrc"
    echo "Until then, use the full path: $bin/buddy"
fi
