#!/bin/sh

set -eu

repository="MCprotein/agent-observability"
default_release_base_url="https://github.com/$repository/releases"
release_base_url=${AGENT_OBSERVABILITY_RELEASE_BASE_URL:-$default_release_base_url}
release_base_url=${release_base_url%/}
install_dir=${AGENT_OBSERVABILITY_INSTALL_DIR:-"$HOME/.local/bin"}
platform=${AGENT_OBSERVABILITY_PLATFORM:-$(uname -s)}

fail() {
  printf 'agent-observability installer: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

case "$platform" in
  Darwin) ;;
  *) fail "macOS is required (detected $platform)" ;;
esac

for command_name in awk chmod curl grep install mkdir mktemp mv rm sed shasum tar; do
  require_command "$command_name"
done

case "$install_dir" in
  /*) ;;
  *) fail "install directory must be an absolute path: $install_dir" ;;
esac
case "$install_dir" in
  *:*) fail "install directory must not contain a colon" ;;
  *'
'*) fail "install directory must not contain a newline" ;;
esac

version=${AGENT_OBSERVABILITY_VERSION:-}
if [ -z "$version" ]; then
  latest_url=$(curl -fsSIL -o /dev/null -w '%{url_effective}' "$release_base_url/latest") ||
    fail "could not resolve the latest release"
  version=${latest_url##*/}
fi
version=${version#v}

case "$version" in
  ''|*[!0-9.]*) fail "invalid release version: $version" ;;
esac
old_ifs=$IFS
IFS=.
set -- $version
IFS=$old_ifs
[ "$#" -eq 3 ] || fail "invalid release version: $version"
for part in "$@"; do
  case "$part" in
    ''|*[!0-9]*) fail "invalid release version: $version" ;;
  esac
done

tag="v$version"
archive_name="agent-observability-$version-darwin-universal2.tar.gz"
archive_root="agent-observability-$version-darwin-universal2"
download_base="$release_base_url/download/$tag"

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/agent-observability-install.XXXXXX") ||
  fail "could not create a temporary directory"
pending_binary=""
cleanup() {
  rm -rf "$work_dir"
  if [ -n "$pending_binary" ]; then
    rm -f "$pending_binary"
  fi
}
trap cleanup EXIT HUP INT TERM

archive_path="$work_dir/$archive_name"
checksums_path="$work_dir/SHA256SUMS"
curl -fsSL "$download_base/$archive_name" -o "$archive_path" ||
  fail "could not download $archive_name"
curl -fsSL "$download_base/SHA256SUMS" -o "$checksums_path" ||
  fail "could not download SHA256SUMS"

expected_checksum=$(awk -v name="$archive_name" '$2 == name { print $1; exit }' "$checksums_path")
[ -n "$expected_checksum" ] || fail "SHA256SUMS does not contain $archive_name"
actual_checksum=$(shasum -a 256 "$archive_path" | awk '{ print $1 }')
[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed"

mkdir "$work_dir/extracted"
tar -xzf "$archive_path" -C "$work_dir/extracted" \
  "$archive_root/agent-observability" || fail "release archive is invalid"
source_binary="$work_dir/extracted/$archive_root/agent-observability"
[ -f "$source_binary" ] && [ ! -L "$source_binary" ] ||
  fail "release archive does not contain a regular executable"
chmod 0755 "$source_binary"
[ "$("$source_binary" --version)" = "$version" ] ||
  fail "downloaded executable version does not match $version"

install -d -m 0755 "$install_dir"
pending_binary="$install_dir/.agent-observability.install.$$"
install -m 0755 "$source_binary" "$pending_binary"
mv -f "$pending_binary" "$install_dir/agent-observability"
pending_binary=""

profile=${AGENT_OBSERVABILITY_SHELL_PROFILE:-}
if [ -z "$profile" ]; then
  case "${SHELL:-}" in
    */zsh) profile="$HOME/.zshrc" ;;
    */bash) profile="$HOME/.bash_profile" ;;
    *) profile="$HOME/.profile" ;;
  esac
fi

case "$profile" in
  *'
'*) fail "shell profile path must not contain a newline" ;;
esac
marker_start="# >>> agent-observability PATH >>>"
marker_end="# <<< agent-observability PATH <<<"
quoted_profile=$(printf '%s' "$profile" | sed "s/'/'\\\\''/g")
has_marker_start=false
has_marker_end=false
if [ -f "$profile" ]; then
  grep -Fxq "$marker_start" "$profile" && has_marker_start=true
  grep -Fxq "$marker_end" "$profile" && has_marker_end=true
fi
[ "$has_marker_start" = "$has_marker_end" ] ||
  fail "shell profile contains an incomplete agent-observability PATH block"
if [ "$has_marker_start" = false ]; then
  profile_dir=${profile%/*}
  [ "$profile_dir" = "$profile" ] || install -d -m 0755 "$profile_dir"
  quoted_install_dir=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
  {
    [ ! -s "$profile" ] || printf '\n'
    printf '%s\n' "$marker_start"
    printf 'case ":$PATH:" in\n'
    printf "  *:'%s':*) ;;\n" "$quoted_install_dir"
    printf '  *) export PATH='"'"'%s'"'"':"$PATH" ;;\n' "$quoted_install_dir"
    printf 'esac\n'
    printf '%s\n' "$marker_end"
  } >> "$profile"
fi

printf 'Installed agent-observability %s to %s\n' "$version" "$install_dir/agent-observability"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) printf "Activate it in this terminal: . '%s'\n" "$quoted_profile" ;;
esac
