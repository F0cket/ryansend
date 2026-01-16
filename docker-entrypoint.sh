#!/bin/bash

# Default values
DEFAULT_PUID=1000
DEFAULT_PGID=1000

# Get PUID and PGID from environment or use defaults
PUID=${PUID:-$DEFAULT_PUID}
PGID=${PGID:-$DEFAULT_PGID}

# Function to log messages
log() {
    echo "[docker-entrypoint] $1"
}

log "Starting ryansend with PUID=$PUID, PGID=$PGID"

# Create group if it doesn't exist
if ! getent group appgroup >/dev/null 2>&1; then
    # Check if GID is already taken
    if getent group "$PGID" >/dev/null 2>&1; then
        EXISTING_GROUP=$(getent group "$PGID" | cut -d: -f1)
        log "GID $PGID already exists for group '$EXISTING_GROUP', using existing group"
        PGID=$(getent group "$EXISTING_GROUP" | cut -d: -f3)
    else
        log "Creating group 'appgroup' with GID $PGID"
        groupadd -g "$PGID" appgroup
    fi
else
    log "Group 'appgroup' already exists"
fi

# Create user if it doesn't exist
if ! getent passwd appuser >/dev/null 2>&1; then
    # Check if UID is already taken
    if getent passwd "$PUID" >/dev/null 2>&1; then
        EXISTING_USER=$(getent passwd "$PUID" | cut -d: -f1)
        log "UID $PUID already exists for user '$EXISTING_USER', creating appuser with next available UID"
        useradd -g "$PGID" -d /data -s /bin/bash appuser 2>/dev/null || true
        PUID=$(id -u appuser)
        log "Created user 'appuser' with UID $PUID"
    else
        log "Creating user 'appuser' with UID $PUID"
        useradd -u "$PUID" -g "$PGID" -d /data -s /bin/bash appuser 2>/dev/null || true
    fi
else
    log "User 'appuser' already exists"
    # Update to match PUID/PGID if different
    CURRENT_UID=$(id -u appuser)
    CURRENT_GID=$(id -g appuser)
    if [ "$CURRENT_UID" != "$PUID" ] || [ "$CURRENT_GID" != "$PGID" ]; then
        log "Updating user 'appuser' to UID $PUID and GID $PGID"
        usermod -u "$PUID" -g "$PGID" appuser 2>/dev/null || true
    fi
fi

# Ensure /data directory exists and has correct permissions
mkdir -p /data
chown -R "$PUID:$PGID" /data

# If config.yaml doesn't exist and we have environment variables,
# we may need to initialize with proper permissions
if [ ! -f /data/config.yaml ]; then
    log "No config.yaml found - ryansend will create one on startup"
fi

# Log some useful information
log "Working directory: /data"
log "Binary location: /usr/local/bin/ryansend"
log "License files: /ryansend/"

# Switch to the application user and run ryansend
log "Switching to user appuser (UID $PUID) and starting ryansend"
exec gosu appuser ryansend "$@"
