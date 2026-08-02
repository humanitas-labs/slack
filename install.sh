#!/bin/sh
# Install the latest slack CLI release binary.
#   curl -fsSL https://raw.githubusercontent.com/digimata/slack/master/install.sh | sh
# Destination defaults to ~/.local/bin; override with SLACK_INSTALL_DIR.
set -eu

repo="digimata/slack"
dir="${SLACK_INSTALL_DIR:-$HOME/.local/bin}"

os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Darwin) sys="apple-darwin" ;;
    Linux)  sys="unknown-linux-musl" ;;
    *) echo "error: unsupported OS: $os (build from source with cargo)" >&2; exit 1 ;;
esac
case "$arch" in
    arm64 | aarch64) cpu="aarch64" ;;
    x86_64 | amd64)  cpu="x86_64" ;;
    *) echo "error: unsupported architecture: $arch (build from source with cargo)" >&2; exit 1 ;;
esac
target="$cpu-$sys"

url="https://github.com/$repo/releases/latest/download/slack-$target.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "downloading slack-$target..."
curl -fsSL "$url" -o "$tmp/slack.tar.gz"
tar -xzf "$tmp/slack.tar.gz" -C "$tmp"

mkdir -p "$dir"
install -m 755 "$tmp/slack" "$dir/slack"
echo "installed $("$dir/slack" --version) to $dir/slack"

case ":$PATH:" in
    *":$dir:"*) ;;
    *) echo "note: $dir is not on your PATH" ;;
esac
