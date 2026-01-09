# ryansend for Unraid

This is the official Unraid Community Applications template for ryansend - a secure file sharing tool that generates cryptographic URLs for temporary file access.

## Quick Setup

1. **Install from Community Applications**
   - Search for "ryansend" in the Apps tab
   - Click Install and configure the settings below

2. **Required Configuration**
   - **Base URL**: Set this to your server's URL (e.g., `http://tower.local:3000` or `https://yourdomain.com:3000`)
   - **Config Directory**: `/mnt/user/appdata/ryansend` (default is fine)
   - **Shared Files**: Point to your media directory (e.g., `/mnt/user/media` or `/mnt/user/`)

3. **Optional Configuration**
   - **Enable Admin Panel**: Set to `true` to get a web interface for browsing and sharing files
   - **Admin Sharing Root**: Directory the admin panel can browse (usually `/shared`)

## Default Setup

The template is pre-configured for typical Unraid usage:

```
Ports:
- 3000: Main ryansend port
- 3001: Admin panel (if enabled)

Volumes:
- /mnt/user/appdata/ryansend -> /app (config storage)
- /mnt/user/ -> /shared (your files, read-only)

Environment:
- Base URL: http://tower.local:3000
- Admin Panel: Enabled
- Admin Root: /shared
```

## First Run

1. Start the container
2. Check the logs for the generated admin password
3. Access the admin panel at `http://your-server-ip:3001/admin/login`
4. Use the password from the logs to log in

## Usage

### Command Line Sharing
```bash
docker exec ryansend ryansend share /shared/path/to/file.iso
```

### Admin Web Interface
- Browse to `http://your-server-ip:3001/admin/login`
- Log in with the generated password
- Browse your files and generate sharing links

## Security Notes

- The shared files directory is mounted read-only for security
- Generated URLs expire automatically (1 hour by default)
- Admin panel requires password authentication
- No permanent public file hosting - links expire

## Changing Admin Password

```bash
docker exec -it ryansend ryansend set-password
```

## Logs

```bash
docker logs ryansend
```

The admin password will be shown in the logs on first startup.

## Support

- GitHub: https://github.com/rlittlefield/ryansend
- Docker Hub: https://hub.docker.com/r/ryanlittlefield/ryansend

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `RYANSEND_BASE_URL` | `http://localhost:3000` | Base URL for generated links |
| `RYANSEND_PORT` | `3000` | Main application port |
| `RYANSEND_ADMIN_PORT` | `3001` | Admin panel port |
| `RYANSEND_DEFAULT_ADMIN_PANEL` | `false` | Enable admin on first run |
| `RYANSEND_ADMIN_SHARING_ROOT` | `/shared` | Admin panel root directory |
| `RYANSEND_REMOVE_KOFI` | `false` | Remove ko-fi support link |
| `RUST_LOG` | `info` | Log level |
