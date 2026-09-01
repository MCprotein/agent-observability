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

for command_name in awk cat chmod cp curl install ln mkdir mktemp mv readlink rm sed shasum tar; do
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
pending_alias=""
pending_profile=""
install_mutating=0
install_complete=0
binary_path="$install_dir/agent-observability"
alias_path="$install_dir/agentobs"
backup_binary="$work_dir/agent-observability.backup"
backup_alias="$work_dir/agentobs.backup"
cleanup() {
  if [ "$install_mutating" -eq 1 ] && [ "$install_complete" -eq 0 ]; then
    rm -f "$binary_path" "$alias_path"
    if [ -e "$backup_binary" ] || [ -L "$backup_binary" ]; then
      mv "$backup_binary" "$binary_path"
    fi
    if [ -e "$backup_alias" ] || [ -L "$backup_alias" ]; then
      mv "$backup_alias" "$alias_path"
    fi
  fi
  rm -rf "$work_dir"
  if [ -n "$pending_binary" ]; then
    rm -f "$pending_binary"
  fi
  if [ -n "$pending_alias" ]; then
    rm -f "$pending_alias"
  fi
  if [ -n "$pending_profile" ]; then
    rm -f "$pending_profile"
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

if [ ! -d "$install_dir" ]; then
  [ ! -e "$install_dir" ] || fail "install path is not a directory: $install_dir"
  install -d -m 0755 "$install_dir"
fi
[ ! -d "$binary_path" ] || fail "command path is a directory: $binary_path"
if [ -e "$alias_path" ] || [ -L "$alias_path" ]; then
  [ -L "$alias_path" ] && [ "$(readlink "$alias_path")" = "agent-observability" ] ||
    fail "command alias is not managed by this installer: $alias_path"
fi
pending_binary="$install_dir/.agent-observability.install.$$"
install -m 0755 "$source_binary" "$pending_binary"
pending_alias="$install_dir/.agentobs.install.$$"
ln -s agent-observability "$pending_alias"

install_mutating=1
if [ -e "$binary_path" ] || [ -L "$binary_path" ]; then
  mv "$binary_path" "$backup_binary"
fi
if [ -e "$alias_path" ] || [ -L "$alias_path" ]; then
  mv "$alias_path" "$backup_alias"
fi
mv "$pending_binary" "$binary_path"
pending_binary=""
mv "$pending_alias" "$alias_path"
pending_alias=""
install_complete=1

profile=${AGENT_OBSERVABILITY_SHELL_PROFILE:-}
if [ -z "$profile" ]; then
  case "${SHELL:-}" in
    */zsh) profile="$HOME/.zshrc" ;;
    */bash) profile="$HOME/.bash_profile" ;;
    *) profile="$HOME/.profile" ;;
  esac
fi

case "$profile" in
  /*) ;;
  *) fail "shell profile must be an absolute path: $profile" ;;
esac
case "$profile" in
  *'
'*) fail "shell profile path must not contain a newline" ;;
esac

profile_target=$profile
link_depth=0
while [ -L "$profile_target" ]; do
  link_depth=$((link_depth + 1))
  [ "$link_depth" -le 16 ] || fail "shell profile symlink chain is too deep"
  link_target=$(readlink "$profile_target") || fail "could not read shell profile symlink"
  case "$link_target" in
    /*) profile_target=$link_target ;;
    *) profile_target=${profile_target%/*}/$link_target ;;
  esac
done
case "$profile_target" in
  *'
'*) fail "shell profile target must not contain a newline" ;;
esac
marker_start="# >>> agent-observability PATH >>>"
marker_end="# <<< agent-observability PATH <<<"
quoted_profile=$(printf '%s' "$profile" | sed "s/'/'\\\\''/g")
marker_state=absent
if [ -f "$profile_target" ]; then
  marker_state=$(awk -v start="$marker_start" -v end="$marker_end" '
    $0 == start {
      if (managed || seen) invalid = 1
      managed = 1
      seen = 1
      next
    }
    $0 == end {
      if (!managed) invalid = 1
      managed = 0
    }
    END {
      if (managed || invalid) print "invalid"
      else if (seen) print "complete"
      else print "absent"
    }
  ' "$profile_target")
fi
case "$marker_state" in
  absent|complete) ;;
  *) fail "shell profile contains an invalid agent-observability PATH block" ;;
esac

profile_dir=${profile_target%/*}
if [ "$profile_dir" != "$profile_target" ] && [ ! -d "$profile_dir" ]; then
  [ ! -e "$profile_dir" ] || fail "shell profile parent is not a directory: $profile_dir"
  install -d -m 0755 "$profile_dir"
fi
quoted_install_dir=$(printf '%s' "$install_dir" | sed "s/'/'\\\\''/g")
path_block="$work_dir/path-block"
{
  printf '%s\n' "$marker_start"
  printf 'case ":$PATH:" in\n'
  printf "  *:'%s':*) ;;\n" "$quoted_install_dir"
  printf '  *) export PATH='"'"'%s'"'"':"$PATH" ;;\n' "$quoted_install_dir"
  printf 'esac\n'
  printf '%s\n' "$marker_end"
} > "$path_block"

profile_update="$work_dir/profile-update"
if [ "$marker_state" = complete ]; then
  awk -v start="$marker_start" -v end="$marker_end" -v block="$path_block" '
    $0 == start {
      while ((getline line < block) > 0) print line
      close(block)
      managed = 1
      next
    }
    managed && $0 == end { managed = 0; next }
    !managed { print }
  ' "$profile_target" > "$profile_update"
else
  if [ -f "$profile_target" ]; then
    cat "$profile_target" > "$profile_update"
  fi
  [ ! -s "$profile_update" ] || printf '\n' >> "$profile_update"
  cat "$path_block" >> "$profile_update"
fi

pending_profile=$(mktemp "$profile_dir/.agent-observability-profile.XXXXXX") ||
  fail "could not create a shell profile update"
if [ -e "$profile_target" ]; then
  cp -p "$profile_target" "$pending_profile"
  cat "$profile_update" > "$pending_profile"
else
  install -m 0600 "$profile_update" "$pending_profile"
fi
mv -f "$pending_profile" "$profile_target"
pending_profile=""

printf 'Installed agentobs %s (alias: agent-observability) to %s\n' "$version" "$install_dir"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) printf "Activate it in this terminal: . '%s'\n" "$quoted_profile" ;;
esac
