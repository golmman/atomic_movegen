FROM mcr.microsoft.com/devcontainers/base:debian-12

ARG DEFAULT_SHELL=
ARG USERNAME=vscode
# UID/GID of the host user — passed in by devcontainer.json so the
# in-container user matches the host. Falls back to 1000 if not set.
ARG USER_UID=1000
ARG USER_GID=1000

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

RUN apt-get update && apt-get install -y --no-install-recommends \
        ripgrep fd-find fzf bat jq tmux \
        python3 python3-venv pipx \
    && ln -sf /usr/bin/fdfind /usr/local/bin/fd \
    && ln -sf /usr/bin/batcat /usr/local/bin/bat \
    && rm -rf /var/lib/apt/lists/*

# Remap the existing `vscode` user to the host's UID/GID and re-own its
# home dir, so bind-mounted host files are writable inside the container
# without any mount-time UID translation tricks (idmap / chown / U).
RUN set -eu \
    && CUR_UID="$(id -u "$USERNAME")" \
    && CUR_GID="$(id -g "$USERNAME")" \
    && if [ "$CUR_GID" != "$USER_GID" ]; then \
         # If a different group already owns the target GID, rename it out of the way.
         EXISTING_GROUP="$(getent group "$USER_GID" | cut -d: -f1 || true)"; \
         if [ -n "$EXISTING_GROUP" ] && [ "$EXISTING_GROUP" != "$USERNAME" ]; then \
           groupmod -g "$((USER_GID + 10000))" "$EXISTING_GROUP"; \
         fi; \
         groupmod -g "$USER_GID" "$USERNAME"; \
       fi \
    && if [ "$CUR_UID" != "$USER_UID" ]; then \
         EXISTING_USER="$(getent passwd "$USER_UID" | cut -d: -f1 || true)"; \
         if [ -n "$EXISTING_USER" ] && [ "$EXISTING_USER" != "$USERNAME" ]; then \
           usermod -u "$((USER_UID + 10000))" "$EXISTING_USER"; \
         fi; \
         usermod -u "$USER_UID" "$USERNAME"; \
       fi \
    && chown -R "$USER_UID:$USER_GID" "/home/$USERNAME"

RUN HOST_SHELL_BASENAME="$(basename "${DEFAULT_SHELL:-}")" \
    && case "$HOST_SHELL_BASENAME" in \
        bash) RESOLVED=bash; SHELL_PATH=/bin/bash ;; \
        zsh)  RESOLVED=zsh;  SHELL_PATH=/bin/zsh  ;; \
        *)    echo "[shell] '$HOST_SHELL_BASENAME' unsupported, falling back to bash"; \
              RESOLVED=bash; SHELL_PATH=/bin/bash ;; \
    esac \
    && usermod -s "$SHELL_PATH" "$USERNAME" \
    && echo "$RESOLVED" > /etc/devcontainer-shell

# Pre-create XDG dirs that named-volume mounts will land inside, so Podman
# doesn't auto-create their parents as root and lock the user out.
RUN install -d -o "$USERNAME" -g "$USERNAME" \
        "/home/$USERNAME/.config" \
        "/home/$USERNAME/.config/opencode" \
        "/home/$USERNAME/.cache" \
        "/home/$USERNAME/.cache/opencode" \
        "/home/$USERNAME/.cache/tmp" \
        "/home/$USERNAME/.local" \
        "/home/$USERNAME/.local/share" \
        "/home/$USERNAME/.local/share/opencode"
