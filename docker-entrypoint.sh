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
    log "Creating group 'appgroup' with GID $PGID"
    groupadd -g "$PGID" appgroup
fi

# Create user if it doesn't exist
if ! getent passwd appuser >/dev/null 2>&1; then
    log "Creating user 'appuser' with UID $PUID"
    useradd -u "$PUID" -g "$PGID" -d /data -s /bin/bash appuser
else
    # Modify existing user to match PUID/PGID
    log "Updating user 'appuser' to UID $PUID and GID $PGID"
    usermod -u "$PUID" -g "$PGID" appuser >/dev/null 2>&1 || true
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
