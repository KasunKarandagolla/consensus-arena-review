#!/usr/bin/env bash
set -u

# Run inside the same logged-in native Linux desktop session that launches
# `npm run tauri dev` from the repository root. This is a host qualification
# aid, not a GUI E2E test and not a product startup workaround.

report_version() {
    local label="$1"
    shift
    local version
    if version=$(timeout 5s "$@" 2>/dev/null); then
        printf '%s=%s\n' "$label" "$version"
    else
        printf '%s=unavailable\n' "$label"
    fi
}

report_version gtk3 pkg-config --modversion gtk+-3.0
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    report_version webkitgtk pkg-config --modversion webkit2gtk-4.1
elif pkg-config --exists webkit2gtk-4.0 2>/dev/null; then
    report_version webkitgtk pkg-config --modversion webkit2gtk-4.0
else
    printf 'webkitgtk=unavailable\n'
fi

if [[ -d /dev/dri ]]; then
    printf 'dev_dri=present\n'
    find /dev/dri -maxdepth 1 -type c -printf 'device=%f\n' 2>/dev/null | sort
else
    printf 'dev_dri=absent\n'
fi

if command -v glxinfo >/dev/null 2>&1; then
    renderer=$(timeout 10s glxinfo -B 2>/dev/null | awk -F: '/OpenGL renderer string:/ {sub(/^[ \t]+/, "", $2); print $2; exit}')
    if [[ -n "$renderer" ]]; then
        printf 'gl_renderer=%s\n' "$renderer"
    else
        printf 'gl_renderer=unavailable-or-timeout\n'
    fi
else
    printf 'gl_renderer=glxinfo-unavailable\n'
fi

if command -v curl >/dev/null 2>&1 && vite_status=$(timeout 5s curl --silent --show-error --fail \
    --output /dev/null --write-out '%{http_code}' \
    http://127.0.0.1:1420/ 2>/dev/null); then
    printf 'vite_http_status=%s\n' "$vite_status"
else
    printf 'vite_http_status=unavailable\n'
fi

if pids=$(pgrep -x consensus-arena 2>/dev/null); then
    printf 'tauri_process_count=%s\n' "$(printf '%s\n' "$pids" | wc -l | tr -d ' ')"
else
    printf 'tauri_process_count=0\n'
fi

# Never print application-log content here: logs may contain user data. An
# optional file reports only aggregate IPC-related/error line counts.
if [[ -n "${ARENA_LOG_FILE:-}" && -r "${ARENA_LOG_FILE}" ]]; then
    awk '
        /IPC|invoke/ { ipc++ }
        /ERROR|error/ { errors++ }
        END { printf "app_log_ipc_related_lines=%d\napp_log_error_lines=%d\n", ipc, errors }
    ' "$ARENA_LOG_FILE"
else
    printf 'app_log_summary=not-requested\n'
fi
