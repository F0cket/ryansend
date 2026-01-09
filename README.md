# ryansend 

ryansend lets you share files on your computer on a public url.

<img width="789" height="474" alt="Screenshot 2026-01-09 at 11 41 15 AM" src="https://github.com/user-attachments/assets/107cb9ad-3786-48bc-82d9-21d3a13dd4a9" />


## How it works

ryansend has a single executable file, that runs in two modes:

1. `ryansend share FILENAME`
2. `ryansend start`

`ryansend share` generates a unique url with a cryptographic, automatically expiring token. Just pass the path to the file you want to share. The path is embedded into the token, which has the fun property of allowing ryansend to work without a database! By default, urls expire after one hour.

`ryansend start` turns on a webserver. A single URL prefix is exposed on port 3000, matching the URLs generated from `ryansend share`. Files are streamed both off disk and out the network, keeping the process from running out of memory under normal usage.

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
* The only external request made is for the ko-fi image link in the admin page, which can be turned off.


## Setup and config

You can just run `ryansend start`, and assuming you run with adequate permissions, ryansend will generate a new config.yaml file with a default base localhost url and a new randomly generated encryption key. You will probably want to then modify the config file and change `base_url` to your IP address or DNS. Ryansend is made for home users, so the best option is to set up DDNS, then forwarding a port in your router to point to whatever local IP ryansend is running on.

If you want to disable the ko-fi link, just set `remove_kofi: false` in the config.yaml.

### Docker

There is a ryansend docker image hosted at dockerhub: https://hub.docker.com/r/ryanlittlefield/ryansend

You can run the image as-is through docker manually, through docker-compose, k8s, or something like Unraid. The best way to get it working fast is to pass some env vars so you don't have to figure out how to modify the config.yaml:

`RYANSEND_BASE_URL` controls the domain prefix for generated URLS (should include the port)

`RYANSEND_PORT` controls the port it will run on.

Currently, you can't control the generation of the secret key through env vars - it is designed to just make one on startup. If you really want to, you could make a new dockerfile using this as the base, or do a trick to mount the config.yaml into the directory.

#### Mounting files to share

If you want to share something, ryansend needs to be able to read it. If the file isn't mounted into the container, this won't work. Use the system you are using to run docker to configure some files from the host. On raw docker or from unraid, you could do something like this:

```
docker run -d --name ryansend -p 3000:3000 -p 3001:3001 -v /mnt/user/:/shared -e RYANSEND_ADMIN_SHARING_ROOT=/shared -e RYANSEND_DEFAULT_ADMIN_PANEL=true -e RUST_LOG=info -e RYANSEND_BASE_URL=https://example.com docker.io/ryanlittlefield/ryansend:1.3.0
```
