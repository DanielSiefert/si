#!/usr/bin/env bash
set -e

# NOTE: when making changes to this file, its execution must remain idempotent, barring force
# updates. It is acceptable for some steps to be wasteful (e.g. re-installing a package without
# checking version) so long as the spirit of an idempotency guarantee remains. This script's
# definition of "idempotency" extends to force updating packages that may already exist, which
# is not truly "idempotent". However, we may make this concession in scenarios where we may have
# to force update a package in lieu of a package manager (e.g. using a "latest" GitHub release).

SI_USER="unknown"
SI_OS="unknown"
SI_WSL2="unknown"
SI_ARCH="unknown"

function die {
    if [ "$1" = "" ]; then
        die "must provide argument <error-message>"
    fi
    echo "error: $1"
    exit 1
}

function determine-user {
    if [ $EUID -eq 0 ]; then
        die "must run as non-root"
    fi
    SI_USER=$(whoami)
    sudo -v
}

function set-config {
    # Since arm64 has many names, we standardize on one to reduce verbosity within this script.
    SI_ARCH="$(uname -m)"
    if [ "$SI_ARCH" = "aarch64" ]; then
        SI_ARCH="arm64"
    fi

    if [ "$(uname -s)" = "Darwin" ]; then
        SI_OS="darwin"
        SI_WSL2="false"
    elif [ "$(uname -s)" = "Linux" ]; then
        if [ -f /etc/os-release ]; then
            SI_OS=$(grep '^ID=' /etc/os-release | sed 's/^ID=//' | tr -d '"')
        else
            die "file \"/etc/os-release\" not found"
        fi

        SI_WSL2="false"
        if [ -f /proc/sys/kernel/osrelease ] && [ $(grep "WSL2" /proc/sys/kernel/osrelease) ]; then
            SI_WSL2="true"
        fi
    else
        die "detected OS is neither Darwin nor Linux"
    fi
}

function darwin-bootstrap {
    local pkgs=(
        automake
        bash
        butane
        git
        kubeval
        libtool
        make
        skopeo
    )

    brew update
    brew upgrade
    brew cleanup
    brew install "${pkgs[@]}"
}

function arch-bootstrap {
    local pkgs=(
        base-devel
        git
        make
        skopeo
        wget
    )

    sudo pacman -Syu --noconfirm "${pkgs[@]}"
    install-kubeval-linux-amd64
    install-butane-linux-amd64
}

function fedora-bootstrap {
    local pkgs=(
        @development-tools
        butane
        git
        golang-github-instrumenta-kubeval
        lld
        make
        skopeo
        wget
    )

    sudo dnf upgrade -y --refresh
    sudo dnf autoremove -y
    sudo dnf install -y "${pkgs[@]}"
}

function ubuntu-bootstrap {
    local pkgs=(
        build-essential
        git
        lld
        make
        skopeo
        wget
    )

    . /etc/os-release
    # Kubic provides skopeo packages for 20.04, but it's available in the main Ubuntu repository in >= 20.10.
    if [ "${VERSION_OD}" == "20.04" ]; then
        echo "deb https://download.opensuse.org/repositories/devel:/kubic:/libcontainers:/stable/xUbuntu_${VERSION_ID}/ /" | sudo tee /etc/apt/sources.list.d/devel:kubic:libcontainers:stable.list
        curl -L https://download.opensuse.org/repositories/devel:/kubic:/libcontainers:/stable/xUbuntu_${VERSION_ID}/Release.key | sudo apt-key add -
    fi

    sudo apt update
    sudo apt upgrade -y
    sudo apt autoremove -y
    sudo apt install -y "${pkgs[@]}"
    install-kubeval-linux-amd64
    install-butane-linux-amd64

    if [ "${SI_WSL2}" == "false" ]; then
        echo "========================================= NOTE ========================================="
        echo "Versions of Docker server <20.10.12 (the currently packaged versions for Ubuntu <= 21.10"
        echo "have issues that cause the networking to become unreliable at moderate levels of"
        echo "concurrency (>= 46 parallel test threads)."
        echo
        echo "Please follow the directions at https://docs.docker.com/engine/install/ubuntu/ to"
        echo "install the latest version of Docker."
        echo "========================================= NOTE ========================================="
    fi
}

function perform-bootstrap {
    if [ "$SI_OS" = "darwin" ] && [ "$SI_ARCH" = "x86_64" ]; then
        darwin-bootstrap
    elif [ "$SI_OS" = "darwin" ] && [ "$SI_ARCH" = "arm64" ]; then
        darwin-bootstrap
    elif [ "$SI_OS" = "arch" ] && [ "$SI_ARCH" = "x86_64" ]; then
        arch-bootstrap
    elif [ "$SI_OS" = "fedora" ] && [ "$SI_ARCH" = "x86_64" ]; then
        fedora-bootstrap
    elif [ "$SI_OS" = "ubuntu" ] && [ "$SI_ARCH" = "x86_64" ]; then
        ubuntu-bootstrap
    else
        die "detected distro \"$SI_OS\" and architecture \"$SI_ARCH\" combination
have not yet been validated

  - if you would like to add this combination, edit \"./scripts/bootstrap.sh\"
    and \"./README.md\" accordingly
  - note: adding your preferred environment will also add you as a maintainer
    of its functionality throughout this repository
  - refer to \"./README.md\" for more information, which includes the formal
    checklist for adding your preferred environment"
    fi
}

function install-kubeval-linux-amd64 {
    if [ -d /tmp/kubeval-download ]; then
        rm -rf /tmp/kubeval-download
    fi
    mkdir -p /tmp/kubeval-download
    wget https://github.com/instrumenta/kubeval/releases/latest/download/kubeval-linux-amd64.tar.gz -P /tmp/kubeval-download
    tar -xf /tmp/kubeval-download/kubeval-linux-amd64.tar.gz --directory /tmp/kubeval-download
    if [ -f /usr/local/bin/kubeval ]; then
        sudo rm -f /usr/local/bin/kubeval
    fi
    sudo mv /tmp/kubeval-download/kubeval /usr/local/bin
    rm -rf /tmp/kubeval-download
}

function install-butane-linux-amd64 {
    if [ -d /tmp/butane-download ]; then
        rm -rf /tmp/butane-download
    fi
    mkdir -p /tmp/butane-download
    wget https://github.com/coreos/butane/releases/latest/download/butane-x86_64-unknown-linux-gnu -O /tmp/butane-download/butane
    chmod +x /tmp/butane-download/butane
    if [ -f /usr/local/bin/butane ]; then
        sudo rm -f /usr/local/bin/butane
    fi
    sudo mv /tmp/butane-download/butane /usr/local/bin
    rm -rf /tmp/butane-download
}

function check-binaries {
    local binaries=(
        butane
        cargo
        docker
        docker-compose
        kubeval
        node
        npm
        skopeo
    )

    for binary in "${binaries[@]}"; do
        if ! [ "$(command -v ${binary})" ]; then
            die "\"$binary\" must be installed and in PATH"
        fi
    done
}

function check-node {
    # Check added due to vercel/pkg requirement: https://github.com/vercel/pkg/issues/838
    if [ "$(node -pe process.release.lts)" = "undefined" ] && [ "$SI_OS" = "darwin" ] && [ "$SI_ARCH" = "arm64" ]; then
        die "must use an LTS release of node for \"$SI_OS\" on \"$SI_ARCH\""
    fi
}


function print-success {
    echo "
Success! Ready to build System Initiative with your config 🦀:

  user  : $SI_USER
  os    : $SI_OS
  arch  : $SI_ARCH
  wsl2  : $SI_WSL2
"
}

determine-user
set-config
perform-bootstrap
check-binaries
check-node
print-success
