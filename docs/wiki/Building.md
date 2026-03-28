# Building & Cross-Compilation

← [Back to Wiki Home](Home)

---

## Quick Start (Native)

```bash
# Build (debug)
cargo build

# Build (release, optimised for size)
cargo build --release

# Run all tests
cargo test

# Run clippy lints
cargo clippy -- -W clippy::all
```

## Cross-Compile for Pi Zero 2W

The Pi Zero 2W uses an aarch64 (ARM64) processor. Cross-compilation from an x86_64 host:

```bash
# Install the target (one-time)
rustup target add aarch64-unknown-linux-gnu

# Install the cross-linker (Debian/Ubuntu)
sudo apt install gcc-aarch64-linux-gnu

# Build
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-gnu
```

The `.cargo/config.toml` in the `rust/` directory pre-configures the linker and sysroot paths:

```toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
rustflags = [
    "-L", "/path/to/pi-sysroot/usr/lib/aarch64-linux-gnu",
    "-C", "link-arg=-Wl,--allow-shlib-undefined",
]
```

## Pi Sysroot for libpcap

The RF classification pipeline depends on libpcap for raw frame capture. Since there's no aarch64 libpcap in standard x86_64 package repos, you need to copy it from a Pi:

```bash
# On the Pi — find the libpcap files
dpkg -L libpcap-dev | grep '\.so\|\.a'

# From your build machine — SCP them to a local sysroot
mkdir -p pi-sysroot/usr/lib/aarch64-linux-gnu
scp pi@10.0.0.2:/usr/lib/aarch64-linux-gnu/libpcap.a pi-sysroot/usr/lib/aarch64-linux-gnu/
scp pi@10.0.0.2:/usr/lib/aarch64-linux-gnu/libpcap.so.1.10.5 pi-sysroot/usr/lib/aarch64-linux-gnu/

# Create the symlink the linker expects
cd pi-sysroot/usr/lib/aarch64-linux-gnu
ln -sf libpcap.so.1.10.5 libpcap.so
```

The `--allow-shlib-undefined` linker flag is needed because libpcap.so dynamically links against libdbus, which isn't in the sysroot. Since dbus is available on the Pi at runtime, we can safely tell the linker to ignore the unresolved symbols.

## WSL Cross-Compilation

If building from Windows via WSL (Windows Subsystem for Linux):

1. Install WSL with Ubuntu
2. Install Rust via rustup inside WSL
3. Follow the cross-compile instructions above (they work identically in WSL)
4. The sysroot path in `.cargo/config.toml` uses `/mnt/c/...` to reference Windows paths from WSL

The MSYS2 environment can also be used, but WSL provides a more complete Linux toolchain.

## Deploy to Pi

```bash
# Build the release binary
cargo build --release --target aarch64-unknown-linux-gnu

# Copy to Pi
scp target/aarch64-unknown-linux-gnu/release/oxigotchi pi@10.0.0.2:/home/pi/

# On the Pi — stop, copy, restart
ssh pi@10.0.0.2 'sudo systemctl stop rusty-oxigotchi && sudo cp /home/pi/oxigotchi /usr/local/bin/ && sudo systemctl restart rusty-oxigotchi'
```

## Install on Existing Pwnagotchi

If you already have a pwnagotchi running on a Pi Zero 2W:

```bash
git clone https://github.com/CoderFX/oxigotchi.git /home/pi/Oxigotchi
cd /home/pi/Oxigotchi/tools
sudo python3 deploy_pwnoxide.py
```

The deployer is an 18-step automated installer. It:
- Backs up your existing firmware before making changes
- Installs the patched firmware
- Installs the Rust binary
- Sets up systemd services
- Migrates your existing pwnagotchi config and captures
- Disables legacy pwnagotchi and bettercap services

## Release Profile

The `Cargo.toml` release profile is tuned for Pi Zero 2W:

| Setting | Value | Purpose |
|---------|-------|---------|
| `opt-level` | `"z"` | Optimise for binary size |
| `lto` | `true` | Link-time optimisation |
| `codegen-units` | `1` | Single codegen unit for better optimisation |
| `strip` | `true` | Strip debug symbols |
| `panic` | `"abort"` | Abort on panic (saves unwinding code) |

The resulting binary is ~5 MB — compared to 150MB+ for the Python/Go stack it replaces.
