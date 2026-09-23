#!/bin/sh
set -eu
parent_pid=$1
target=$2
payload=$3
stage=$4
drawing=$5
platform=$6
reopen() {
    if [ "$platform" = macos ]; then
        if [ -n "$drawing" ]; then /usr/bin/open -n "$target" --args --open "$drawing"; else /usr/bin/open -n "$target"; fi
    else
        if [ -n "$drawing" ]; then "$target/reshiki" --open "$drawing" </dev/null >/dev/null 2>&1 &
        else "$target/reshiki" </dev/null >/dev/null 2>&1 & fi
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
        reopen
    }
    trap 'rollback' EXIT
    /bin/mv "$target" "$backup"
    /bin/mv "$payload" "$target"
    reopen
    trap - EXIT
else
    mkdir "$stage/previous" "$stage/installed"
    rollback() {
        for name in reshiki reshiki-inchi-helper Licenses build.json README.txt; do
            if [ -e "$stage/installed/$name" ]; then rm -rf "$target/$name"; fi
            if [ -e "$stage/previous/$name" ]; then /bin/mv "$stage/previous/$name" "$target/$name"; fi
        done
        reopen
    }
    trap 'rollback' EXIT
    for name in reshiki reshiki-inchi-helper Licenses build.json README.txt; do
        [ -e "$payload/$name" ] || continue
        if [ -e "$target/$name" ]; then /bin/mv "$target/$name" "$stage/previous/$name"; fi
        touch "$stage/installed/$name"
        /bin/mv "$payload/$name" "$target/$name"
    done
    reopen
    trap - EXIT
fi
# Retain the previous application and log for recovery. No user files are deleted.
echo 'Update installed and application restarted.'
