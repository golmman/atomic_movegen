#!/usr/bin/env bash
set -euo pipefail

echo "[post-create] running as $(id)"
echo "[post-create] HOME=${HOME} (owned by uid $(stat -c %u "${HOME}") gid $(stat -c %u "${HOME}"))"

echo "[post-create] installing opencode-ai..."
# Use a user-owned TMPDIR. opencode's postinstall does mkdtemp() and EACCES'es
# on /tmp under some Podman userns configurations.
export TMPDIR="${HOME}/.cache/tmp"
mkdir -p "${TMPDIR}"
npm install -g opencode-ai@latest

if [ -r /etc/devcontainer-shell ]; then
  SHELL_CHOICE="$(cat /etc/devcontainer-shell)"
else
  SHELL_CHOICE="bash"
fi
echo "[post-create] login shell: ${SHELL_CHOICE}"

case "${SHELL_CHOICE}" in
  zsh)
    if [ ! -f "${HOME}/.zshrc" ] || ! grep -q 'devcontainer-seed' "${HOME}/.zshrc"; then
      cat >> "${HOME}/.zshrc" <<'ZSHRC'
# devcontainer-seed
export TMPDIR="${HOME}/.cache/tmp"
export PATH="${HOME}/.local/bin:${PATH}"
alias ll='ls -lah'
alias oc='opencode'
autoload -Uz compinit && compinit -i
PROMPT='%F{cyan}%n@devcontainer%f %F{yellow}%~%f %# '
ZSHRC
    fi
    ;;
  bash)
    if [ ! -f "${HOME}/.bashrc" ] || ! grep -q 'devcontainer-seed' "${HOME}/.bashrc"; then
      cat >> "${HOME}/.bashrc" <<'BASHRC'
# devcontainer-seed
export TMPDIR="${HOME}/.cache/tmp"
export PATH="${HOME}/.local/bin:${PATH}"
alias ll='ls -lah'
alias oc='opencode'
[ -r /usr/share/bash-completion/bash_completion ] && . /usr/share/bash-completion/bash_completion
PS1='\[\033[36m\]\u@devcontainer\[\033[0m\] \[\033[33m\]\w\[\033[0m\] \$ '
BASHRC
    fi
    ;;
esac

mkdir -p "${HOME}/.config/opencode"
if [ ! -f "${HOME}/.config/opencode/opencode.json" ] && [ -f "/workspace/.devcontainer/templates/opencode.json" ]; then
  cp "/workspace/.devcontainer/templates/opencode.json" "${HOME}/.config/opencode/opencode.json"
  echo "[post-create] seeded default opencode.json"
fi

if [ -f "/workspace/.devcontainer/scripts/project-setup.sh" ]; then
  echo "[post-create] running project-setup.sh"
  bash /workspace/.devcontainer/scripts/project-setup.sh || \
    echo "[post-create] project-setup.sh failed (continuing)"
fi

echo "[post-create] done. Run 'opencode' to start."
