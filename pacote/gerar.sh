#!/usr/bin/env bash
# Gera os instaladores do FeatherDB em pacote/saida/, com os nomes fixos que o
# launcher procura na release (releases/latest/download/<nome>):
#   featherdb-linux-x86_64.tar.gz   Linux portátil (instalar.sh na pasta do usuário)
#   featherdb-linux-amd64.deb       Debian, Ubuntu, Mint…
#   FeatherDB-windows-x64.exe       Windows 10/11 (instalador em Program Files\FeatherWorks, pede administrador)
#   FeatherDB-windows-x64-portatil.zip  Windows sem instalar (descompactar e abrir o featherdb.exe)
#   SHA256SUMS
#
# Uso: pacote/gerar.sh [linux|windows ...]   (sem argumentos = todos)
# Precisa de: cargo, dpkg-deb, objdump; podman ou docker para o Linux (sem eles compila aqui,
# e o .deb só instala em distros com a glibc desta máquina); para Windows, o alvo
# x86_64-pc-windows-gnu, mingw-w64, makensis e zip.
set -euo pipefail

pacote="$(cd "$(dirname "$0")" && pwd)"
raiz="$(dirname "$pacote")"
alvo="${CARGO_TARGET_DIR:-$raiz/target}"
versao="$(sed -n 's/^version = "\(.*\)"/\1/p' "$raiz/Cargo.toml" | head -1)"
saida="$pacote/saida"
alvos=("$@")
[[ ${#alvos[@]} -eq 0 ]] && alvos=(linux windows)
rm -rf "$saida" && mkdir -p "$saida"
echo "FeatherDB $versao → $saida"

# O binário pede a glibc de onde foi compilado: num Debian 12 (2.36) ele roda no Ubuntu 22.04+,
# Debian 12+ e Mint 21+. Compilado aqui pediria a glibc desta máquina.
compilar_linux() {
  local motor; motor="$(command -v podman || command -v docker || true)"
  if [[ -z $motor ]]; then
    echo "Sem podman nem docker: compilando aqui (o .deb vai pedir a glibc desta máquina)." >&2
    cargo build --release -p featherdb --manifest-path "$raiz/Cargo.toml"
    bin="$alvo/release/featherdb"
    return
  fi
  # A imagem com o Rust sai uma vez só; o registro de crates fica guardado num volume.
  if ! "$motor" image exists featherdb-debian12 2>/dev/null && ! "$motor" image inspect featherdb-debian12 >/dev/null 2>&1; then
    "$motor" build -t featherdb-debian12 -f - "$pacote" <<'IMG'
FROM docker.io/library/debian:12
RUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev curl ca-certificates && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH=/root/.cargo/bin:$PATH
IMG
  fi
  "$motor" run --rm -v "$raiz":/src:z -v featherdb-registro:/root/.cargo/registry -e CARGO_TARGET_DIR=/src/target/debian12 -e CARGO_BUILD_JOBS -w /src \
    featherdb-debian12 cargo build --release -p featherdb
  bin="$raiz/target/debian12/release/featherdb"
}

linux() {
  local bin; compilar_linux
  # A glibc mínima é a maior versão que o binário usa.
  local glibc; glibc="$(objdump -T "$bin" | grep -o 'GLIBC_2\.[0-9]*' | sort -V | tail -1 | cut -d_ -f2)"
  echo "glibc mínima: $glibc"

  # .deb
  local d; d="$(mktemp -d)"
  install -Dm755 "$bin" "$d/usr/bin/featherdb"
  install -Dm644 "$pacote/featherdb.desktop" "$d/usr/share/applications/featherdb.desktop"
  for t in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$pacote/icones/featherdb-$t.png" "$d/usr/share/icons/hicolor/${t}x${t}/apps/featherdb.png"
  done
  install -Dm644 "$pacote/icones/featherdb.svg" "$d/usr/share/icons/hicolor/scalable/apps/featherdb.svg"
  install -Dm644 "$raiz/LICENSE" "$d/usr/share/doc/featherdb/copyright"
  local tamanho; tamanho="$(du -sk "$d/usr" | cut -f1)"
  mkdir -p "$d/DEBIAN"
  cat > "$d/DEBIAN/control" <<CTL
Package: featherdb
Version: $versao
Section: database
Priority: optional
Architecture: amd64
Installed-Size: $tamanho
Depends: libc6 (>= $glibc), libxkbcommon0, libwayland-client0 | libx11-6, libfontconfig1
Recommends: libvulkan1 | libgl1, unixodbc
Maintainer: FeatherWorks <https://github.com/feather-works>
Homepage: https://github.com/feather-works/featherdb
Description: Cliente de banco de dados da suíte FeatherWorks
 Cliente de banco de dados leve em Rust: PostgreSQL, MySQL/MariaDB,
 SQLite, MongoDB e qualquer banco por ODBC.
CTL
  cat > "$d/DEBIAN/postinst" <<'SH'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
SH
  cp "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  chmod 755 "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  dpkg-deb --root-owner-group --build "$d" "$saida/featherdb-linux-amd64.deb" >/dev/null
  rm -rf "$d"

  # .tar.gz portátil: "<app>-<versão>/instalar.sh", o mesmo que o launcher roda.
  local t; t="$(mktemp -d)/featherdb-$versao"
  mkdir -p "$t"
  cp "$bin" "$t/featherdb"
  cp "$pacote/featherdb.desktop" "$raiz/LICENSE" "$t/"
  cp -r "$pacote/icones" "$t/"
  cat > "$t/instalar.sh" <<'SH'
#!/bin/sh
# Instala o FeatherDB só para este usuário (~/.local), sem administrador.
set -e
cd "$(dirname "$0")"
mkdir -p ~/.local/bin ~/.local/share/applications
install -m755 featherdb ~/.local/bin/featherdb
sed "s#^Exec=featherdb#Exec=$HOME/.local/bin/featherdb#" featherdb.desktop > ~/.local/share/applications/featherdb.desktop
for t in 16 24 32 48 64 128 256 512; do
  mkdir -p ~/.local/share/icons/hicolor/${t}x${t}/apps
  cp icones/featherdb-$t.png ~/.local/share/icons/hicolor/${t}x${t}/apps/featherdb.png
done
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp icones/featherdb.svg ~/.local/share/icons/hicolor/scalable/apps/featherdb.svg
update-desktop-database ~/.local/share/applications 2>/dev/null || true
echo "FeatherDB instalado. Procure por \"FeatherDB\" no menu de aplicativos."
SH
  chmod 755 "$t/instalar.sh"
  tar -C "$(dirname "$t")" -czf "$saida/featherdb-linux-x86_64.tar.gz" "featherdb-$versao"
  rm -rf "$(dirname "$t")"
}

windows() {
  cargo build --release -p featherdb --target x86_64-pc-windows-gnu --manifest-path "$raiz/Cargo.toml"
  local exe="$alvo/x86_64-pc-windows-gnu/release/featherdb.exe"
  x86_64-w64-mingw32-strip "$exe" 2>/dev/null || true
  (cd "$pacote" && makensis -V2 -DVERSAO="$versao" -DEXE="$exe" -DSAIDA="$saida/FeatherDB-windows-x64.exe" featherdb.nsi)
  # Portátil: o mesmo .exe numa pasta FeatherDB, sem instalar nem pedir administrador.
  local t; t="$(mktemp -d)"
  mkdir "$t/FeatherDB" && cp "$exe" "$t/FeatherDB/featherdb.exe"
  rm -f "$saida/FeatherDB-windows-x64-portatil.zip"
  (cd "$t" && zip -qr9 "$saida/FeatherDB-windows-x64-portatil.zip" FeatherDB)
  rm -rf "$t"
}

for a in "${alvos[@]}"; do
  echo "== $a"
  "$a"
done
(cd "$saida" && sha256sum -- * | grep -v SHA256SUMS > SHA256SUMS)
ls -lh "$saida"
