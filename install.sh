#!/usr/bin/env bash
set -euo pipefail

# gitfoam installer — fetches the latest release binary for this platform and
# drops it at ~/.local/bin/gitfoam.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/The-Cloud-Clockwork/gitfoam/main/install.sh | sh

REPO="The-Cloud-Clockwork/gitfoam"
BIN_NAME="gitfoam"
INSTALL_DIR="${GITFOAM_INSTALL_DIR:-$HOME/.local/bin}"

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Linux)   os="linux" ;;
        Darwin)  os="darwin" ;;
        *) echo "unsupported OS: $os" >&2; exit 1 ;;
    esac
    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) echo "unsupported arch: $arch" >&2; exit 1 ;;
    esac
    echo "${os}-${arch}"
}

main() {
    local target url tmp
    target="$(detect_target)"
    url="https://github.com/${REPO}/releases/latest/download/${BIN_NAME}-${target}"
    echo "gitfoam installer"
    echo "  target: ${target}"
    echo "  url:    ${url}"
    echo "  dest:   ${INSTALL_DIR}/${BIN_NAME}"

    mkdir -p "${INSTALL_DIR}"
    tmp="$(mktemp -t gitfoam.XXXXXX)"
    trap 'rm -f "${tmp}"' EXIT

    if ! curl -fL --progress-bar -o "${tmp}" "${url}"; then
        echo "download failed: ${url}" >&2
        exit 1
    fi

    chmod +x "${tmp}"
    mv "${tmp}" "${INSTALL_DIR}/${BIN_NAME}"

    echo
    echo "installed ${INSTALL_DIR}/${BIN_NAME}"
    echo
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo "NOTE: ${INSTALL_DIR} is not on your PATH"
            echo "  add to your shell rc:  export PATH=\"\$HOME/.local/bin:\$PATH\""
            ;;
    esac
    echo
    echo "next steps:"
    echo "  gitfoam add <repo-path> --target gitfoam/\$(hostname)/\$(git -C <repo> branch --show-current)"
    echo "  gitfoam daemon"
}

main "$@"
