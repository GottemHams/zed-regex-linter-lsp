#!/bin/bash
skripdir="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

cd "$skripdir" || exit $?

echo_err() {
	echo >&2 "$@"
}

seddem() {
	# We're gonna need to pass an empty string to `-i` for macOS and any other BSD variant
	# This is actually also supported on Linux but that requires **no space** before the empty string, while Mac/BSD **do require** the space so we're kinda fucked =]
	# Rather than trying to figure it out from `$OSTYPE` or `uname`, we'll just try both variants and suppress errors on the first (since both failing realistically shouldn't happen)
	sed -i '' "$@" 2>/dev/null || sed -i "$@"
}

upd00t_major=false
upd00t_minor=false
upd00t_patch=false
while getopts ':Mmp' opt; do
	case "$opt" in
		M)
			upd00t_major=true
			;;
		m)
			upd00t_minor=true
			;;
		p)
			upd00t_patch=true
			;;
		\?) # Need a literal ? and not the single-token wildcard
			echo_err "[ERROR] Unknown option: -$OPTARG"
			exit 1
			;;
		*)
			echo_err '[ERROR] Unknown error when parsing options'
			exit 1
			;;
	esac
done

if ! $upd00t_major && ! $upd00t_minor && ! $upd00t_patch; then
	echo_err "Usage: $0 [-M] [-m] [-p]"
	echo_err "	-M Increment major version"
	echo_err "	-m Increment minor version"
	echo_err "	-p Increment patch version"
	echo_err ''
	echo_err 'At least one of the options must be specified, which can also be combined to increment the version parts in a cascading manner.'
	echo_err 'Every higher tier increment (e.g. major) will reset the lower tiers (minor and patch) to 0 before potentially incrementing those in turn.'
	echo_err 'For example, -Mmp with a current version of 0.0.5 will result in 1.1.1.'
	exit 1
fi

# We'll just make `extension.toml` the main config file from which the current version is read
elconfigs=(
	extension.toml
	Cargo.toml
	lsp-server/Cargo.toml
)

extension_config="${elconfigs[0]}"
if [[ ! -f $extension_config ]]; then
	echo_err "[ERROR] Main extension config file not found: $extension_config"
	exit 1
fi

# Some versions of Bash may not support `readarray`, so let's do it the old-fashioned way xd
old_version="$(grep -E '^version\s*=\s*".*"' "$extension_config" | head -n 1 | cut -d'"' -f2)"
IFS='.' read -r -a old_version_parts <<< "$old_version"
if [[ ${#old_version_parts[@]} -ne 3 ]]; then
	echo_err "[ERROR] Invalid version format in $extension_config: $old_version"
	exit 1
fi

old_major="${old_version_parts[0]}"
old_minor="${old_version_parts[1]}"
old_patch="${old_version_parts[2]}"
if [[ ! $old_major =~ ^[0-9]+$ || ! $old_minor =~ ^[0-9]+$ || ! $old_patch =~ ^[0-9]+$ ]]; then
	echo_err "[ERROR] Version components must be numeric in $extension_config: $old_version"
	exit 1
fi

new_major="$old_major"
new_minor="$old_minor"
new_patch="$old_patch"
if $upd00t_major; then
	((new_major++))
	new_minor=0
	new_patch=0
fi

if $upd00t_minor; then
	((new_minor++))
	new_patch=0
fi

if $upd00t_patch; then
	((new_patch++))
fi

new_version="${new_major}.${new_minor}.${new_patch}"
echo "Going to update el version from $old_version to $new_version"
echo ''
for elconfig in "${elconfigs[@]}"; do
	# We'll exit eagerly to avoid the config files getting out of sync
	if [[ ! -f $elconfig ]]; then
		echo_err "[ERROR] Config file not found: $elconfig"
		exit 1
	fi

	if ! seddem -E "s/^(version[[:space:]]=[[:space:]])\"[^\"]*\"/\1\"$new_version\"/" "$elconfig"; then
		echo_err "[ERROR] Failed to update version in $elconfig"
		exit 1
	fi

	if [[ $elconfig == *'Cargo.toml' ]]; then
		! cargo update --manifest-path "$elconfig" && echo_err "[WARNING] Failed to update Cargo.lock for $elconfig"
	fi
done

echo ''
echo 'Ayyy should be gucci mane'
echo "Don't forget to run this after committing your other changes:"
echo "git tag -a \"v$new_version\" -m \"Release v$new_version\""
echo "git push && git push --tags"
