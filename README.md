[![CircleCI](https://dl.circleci.com/status-badge/img/gh/rlittlefield/ryansend/tree/main.svg?style=svg)](https://dl.circleci.com/status-badge/redirect/gh/rlittlefield/ryansend/tree/main)
[![DockerHub](https://img.shields.io/docker/v/ryanlittlefield/ryansend/latest)](https://hub.docker.com/r/ryanlittlefield/ryansend)

# ryansend 

ryansend lets you securely share files on your computer via a public url, one at a time

<img width="789" height="474" alt="Screenshot 2026-01-09 at 11 41 15 AM" src="https://github.com/user-attachments/assets/107cb9ad-3786-48bc-82d9-21d3a13dd4a9" />


## How it works

ryansend has a single executable file, that runs in two modes:

1. `ryansend share FILENAME`
2. `ryansend start`

`ryansend share` generates a unique url with a cryptographic, automatically expiring token. Just pass the path to the file you want to share. The path is embedded into the token, which has the fun property of allowing ryansend to work without a database! By default, urls expire after one hour.

`ryansend start` turns on a webserver. A single URL prefix is exposed on port 3000, matching the URLs generated from `ryansend share`. Files are streamed both off disk and out the network, keeping the process from running out of memory under normal usage. If you configure the admin interface, it will generate a unique password and also opens port 3001, where you can use a simple admin file sharing UI to browse, search, and generate sharing URLs.

## What is this for?

* You want something free (doesn't even cost money to disable the ko-fi link)
* You need something that can send 80gb files
* If you are running a home server and want to share files directly
* You don't want your friends needing special software or accounts or VPNs
* You want to send specific files but don't want to permanently host them publicly
* You need something simple and low footprint
  * ryansend docker image is ~32MB compressed
  * executable binary is ~6MB
  * idles at 0% CPU and 8MB of memory usage when nobody is using it
* Doesn't need javascript in the browser so the page renders fast
* The only external request made is for the ko-fi image link in the admin page, which can be turned off. Otherwise there is no tracking!
* No complex storage or database requirements! ryansend only uses a single small `config.yaml` file for config and has no additional storage requirements.

## Setup and config

You can just run `ryansend start`, and assuming you run with adequate permissions, ryansend will generate a new config.yaml file with a default base localhost url and a new randomly generated encryption key. You will probably want to then modify the config file and change `base_url` to your IP address or DNS. Ryansend is made for home users, so the best option is to set up DDNS, then forwarding a port in your router to point to whatever local IP ryansend is running on.

If you want to disable the ko-fi link, just set `remove_kofi: false` in the config.yaml.

### Unraid

You don't need to use unraid to use ryansend, but it does work great! If you just want docker, skip to the docker section below.

If you want to set up unraid, there are a few steps. First, ryansend isn't in the unraid app store yet, so you'll need to manually install the xml template:

```
curl https://raw.githubusercontent.com/rlittlefield/unraid-repository/refs/heads/main/ryansend.xml > /boot/config/plugins/dockerMan/templates-user
```

Now you can easily configure ryansend by going to the Docker menu in Unraid, then clicking the orange "Add Container" button on the bottom left of the page.

It should load a bunch of settings for you. The main thing to change are:
1. Main and Admin ports. The defaults might be fine for you, but you'll want to match it with whatever you plan on exposing through your port forwarding or firewall.
2. Mounts - the default mounts might be fine, but you may want to change the `Shared Files` to point to a more specific directory if you don't want the whole `/mnt/user` to be visible in the admin share page.
3. Base URL - this needs to be your IP address or a domain (DDNS works great), including the port number you set on the "main" port (unless you are doing some port forwarding)

#### Cloudflare

If you do cloudflare, you can use either of these to get DNS set up (if you own/buy a domain and run it through cloudflare):
1. https://github.com/IPGPrometheus/Cloudflared-Unraid (this is great because it can provide https and forward to port 3000!)
2. https://github.com/oznu/docker-cloudflare-ddns or a similar ddns image

#### Tailscale

Unraid tailscale works great. You'll need to set up tailscale on your own using the recommended tailscale stuff for unraid. You can check the box for "Use Tailscale", and then you'll need to choose a tailscale hostname. It will default to port 3000, but I think you should change it to 3001 because the admin interface can then be used by your phone or other tailscale device to share files.

### Docker

There is a ryansend docker image hosted at dockerhub: https://hub.docker.com/r/ryanlittlefield/ryansend

You can run the image as-is through docker manually, through docker-compose, k8s, or something like Unraid. The best way to get it working fast is to pass some env vars so you don't have to figure out how to modify the config.yaml:

`RYANSEND_BASE_URL` controls the domain prefix for generated URLS (should include the port)

`RYANSEND_PORT` controls the port it will run on.

Currently, you can't control the generation of the secret key through env vars - it is designed to just make one on startup. If you really want to, you could make a new dockerfile using this as the base, or do a trick to mount the config.yaml into the directory.

#### Mounting files to share

If you want to share something, ryansend needs to be able to read it. If the file isn't mounted into the container, this won't work. Use the system you are using to run docker to configure some files from the host. On raw docker or from unraid, you could do something like this:

```
docker run -d --name ryansend -p 3000:3000 -p 3001:3001 -v /mnt/user/appdata/ryansend:/data -v /mnt/user/:/shared -e RYANSEND_ADMIN_SHARING_ROOT=/shared -e RYANSEND_DEFAULT_ADMIN_PANEL=true -e RUST_LOG=info -e RYANSEND_BASE_URL=https://example.com docker.io/ryanlittlefield/ryansend:1.7.1
```

This example works great on unraid, but should work in many docker setups. It will set up two mounts to the host: the appdata one so your config.yaml persists even if you upgrade the image later, and the /mnt/user as the root shared directory. You will probably want to adjust that to something like `/mnt/user/media` or something if you don't really want the admin interface to be bothered with data from random other apps.

## TLS / HTTPS

There are three suggested ways to get secure connections:

- **cloudflared**: If you do cloudflare, you can use something like Cloudflared-Unraid, or a similar manual setup with cloudflared.
  - This is often really easy!
  - No need to forward ports at the router! - Great for security or if you have something using those ports already.
  - Downsides: vague speed limit (probably not as fast as direct connection)
- **lets-encrypt**: ryansend now has beta support for automatic cert management!
  - The new admin setup page lets you check a box and it will try to provision a cert matching your base URL.
  - You have to point your DNS at your up, then forward your public IP address's port 80 to ryansend's main port first.
  - Automatic cert renewal is automatically handled! (currently alpha)
  - Its a little finicky. May require you to change ports, then press the restart button, _then_ click the box to start the lets-encrypt process. Not a lot of visibility if any issues happen.
- **manual**: Drop a cert.pem and key.pem file in the appdata directory and configure a TLS port!
  - Great if you want faster speed than cloudflared, but want to use a different provider or self signed certs.
  - If you want to use manual SSL certificates, you can use a tool like [certbot](https://certbot.eff.org/) or [acme.sh](https://github.com/acmesh-official/acme.sh).


## Contributing

We welcome contributions from the community! To contribute to ryansend, please:

1. **Sign off your commits** - This project uses the [Developer Certificate of Origin (DCO)](DCO) to ensure proper licensing of contributions
2. **Use `git commit -s`** to automatically add your sign-off line
3. **Follow the guidelines** in [CONTRIBUTING.md](CONTRIBUTING.md)

All contributions must include a `Signed-off-by` line to certify that you have the right to submit your changes under the project's MIT license.

## License

This project is licensed under the [MIT License](LICENSE).
