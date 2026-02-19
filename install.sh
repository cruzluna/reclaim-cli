#!/usr/bin/env bash
set -euo pipefail

BIN_NAME="reclaim"
REPO="cruzluna/reclaim-cli"
INSTALL_DIR="${RECLAIM_INSTALL_DIR:-$HOME/.local/bin}"
INSTALL_TAG="${RECLAIM_INSTALL_TAG:-latest}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

is_amazon_linux_2() {
  if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    if [[ "${ID:-}" == "amzn" && ( "${VERSION_ID:-}" == "2" || "${VERSION_ID:-}" == 2.* ) ]]; then
      return 0
    fi
  fi

  if [[ -r /etc/system-release ]] && grep -qi "Amazon Linux release 2" /etc/system-release; then
    return 0
  fi

  return 1
}

require_cmd curl
require_cmd tar
require_cmd uname

uname_s="$(uname -s)"
uname_m="$(uname -m)"

case "$uname_s" in
  Darwin) os="apple-darwin" ;;
  Linux)
    os="unknown-linux-gnu"
    if [[ -f /etc/alpine-release ]]; then
      os="unknown-linux-musl"
    fi
    ;;
  *)
    echo "Unsupported OS: $uname_s" >&2
    exit 1
    ;;
esac

case "$uname_m" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *)
    echo "Unsupported architecture: $uname_m" >&2
    exit 1
    ;;
esac

asset_suffix=""
target="${arch}-${os}"
if [[ -n "${RECLAIM_INSTALL_TARGET:-}" ]]; then
  target="${RECLAIM_INSTALL_TARGET}"
fi

if [[ "$target" == *"-al2" ]]; then
  target="${target%-al2}"
  if [[ -z "${RECLAIM_INSTALL_ASSET_SUFFIX:-}" ]]; then
    asset_suffix="-al2"
  fi
fi

if [[ -n "${RECLAIM_INSTALL_ASSET_SUFFIX:-}" ]]; then
  asset_suffix="${RECLAIM_INSTALL_ASSET_SUFFIX}"
elif [[ "$target" == *"-unknown-linux-gnu" ]] && is_amazon_linux_2; then
  asset_suffix="-al2"
fi

archive="reclaim-cli-${target}${asset_suffix}.tar.gz"
if [[ "$INSTALL_TAG" == "latest" ]]; then
  url="https://github.com/${REPO}/releases/latest/download/${archive}"
else
  url="https://github.com/${REPO}/releases/download/${INSTALL_TAG}/${archive}"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Downloading ${url}"
curl -fsSL "${url}" -o "${tmp_dir}/${archive}"
tar -xzf "${tmp_dir}/${archive}" -C "${tmp_dir}"

bin_path=""
if [[ -f "${tmp_dir}/${BIN_NAME}" ]]; then
  bin_path="${tmp_dir}/${BIN_NAME}"
else
  for candidate in "${tmp_dir}"/reclaim-cli-*/"${BIN_NAME}"; do
    if [[ -f "${candidate}" ]]; then
      bin_path="${candidate}"
      break
    fi
  done
fi

if [[ -z "${bin_path}" ]]; then
  echo "Unable to locate ${BIN_NAME} in downloaded archive ${archive}." >&2
  echo "Try setting RECLAIM_INSTALL_TARGET to an explicit target." >&2
  exit 1
fi

mkdir -p "${INSTALL_DIR}"
install -m 0755 "${bin_path}" "${INSTALL_DIR}/${BIN_NAME}"

echo "Installed ${BIN_NAME} to ${INSTALL_DIR}/${BIN_NAME}"

if ! command -v "${BIN_NAME}" >/dev/null 2>&1; then
  echo "Make sure ${INSTALL_DIR} is on your PATH."
fi
