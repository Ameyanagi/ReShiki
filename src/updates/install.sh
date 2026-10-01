#!/bin/sh
set -eu
parent_pid=$1
target=$2
payload=$3
stage=$4
drawing=$5
platform=$6
shift 6
reopen() {
    # Convert the additional paths to --open pairs without evaluating shell text.
    for path do
        shift
        set -- "$@" --open "$path"
    done
    if [ -n "$drawing" ]; then set -- --open "$drawing" "$@"; fi
    if [ "$platform" = macos ]; then
        /usr/bin/open -n "$target" --args "$@"
    else
        "$target/reshiki" "$@" </dev/null >/dev/null 2>&1 &
    fi
}
touch "$stage/ready"
attempt=0
while kill -0 "$parent_pid" 2>/dev/null; do
    attempt=$((attempt+1))
    if [ "$attempt" -gt 60 ]; then echo 'Application did not exit; nothing installed.'; exit 1; fi
    sleep 1
done
if [ "$platform" = macos ]; then
    backup="$stage/previous.app"
    rollback() {
        if [ -e "$backup" ]; then
            if [ -e "$target" ]; then /bin/mv "$target" "$stage/failed.app"; fi
            /bin/mv "$backup" "$target"
        fi
        reopen "$@"
    }
    trap 'rollback "$@"' EXIT
    /bin/mv "$target" "$backup"
    /bin/mv "$payload" "$target"
    reopen "$@"
    trap - EXIT
else
    mkdir "$stage/previous" "$stage/installed"
    rollback() {
        for name in reshiki reshiki-inchi-helper Licenses build.json README.txt; do
            if [ -e "$stage/installed/$name" ]; then rm -rf "$target/$name"; fi
            if [ -e "$stage/previous/$name" ]; then /bin/mv "$stage/previous/$name" "$target/$name"; fi
        done
        reopen "$@"
    }
    trap 'rollback "$@"' EXIT
    if [ ! -f "$payload/reshiki" ]; then
        echo 'Update payload is missing its application; nothing installed.' >&2
        exit 1
    fi
    for name in reshiki reshiki-inchi-helper Licenses build.json README.txt; do
        # Retire an old owned helper even when the new package has no companion.
        # Keep it in the rollback directory until the replacement has succeeded.
        # Preserve optional paths if their replacement has disappeared. A lost
        # application executable must fail its move and trigger rollback.
        [ -e "$payload/$name" ] || [ "$name" = reshiki ] || [ "$name" = reshiki-inchi-helper ] || continue
        if [ -e "$target/$name" ]; then /bin/mv "$target/$name" "$stage/previous/$name"; fi
        if [ "$name" = reshiki ] || [ -e "$payload/$name" ]; then
            touch "$stage/installed/$name"
            /bin/mv "$payload/$name" "$target/$name"
        fi
    done
    reopen "$@"
    trap - EXIT
fi
# Retain the previous application and log for recovery. No user files are deleted.
echo 'Update installed and application restarted.'
