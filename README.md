# ryansend 

ryansend lets you share files on your computer on a public url.

## How it works

ryansend has a single executable file, that runs in two modes:

1. `ryansend share FILENAME`
2. `ryansend start`

`ryansend share` generates a unique url with a cryptographic, automatically expiring token. Just pass the path to the file you want to share. The path is embedded into the token, which has the fun property of allowing ryansend to work without a database! By default, urls expire after one hour.

`ryansend start` turns on a webserver. A single URL prefix is exposed on port 3000, matching the URLs generated from `ryansend share`. Files are streamed both off disk and out the network, keeping the process from running out of memory under normal usage.

## Setup and config

You can just run `ryansend start`, and assuming you run with adequate permissions, ryansend will generate a new config.yaml file with a default base localhost url and a new randomly generated encryption key. You will probably want to then modify the config file and change `base_url` to your IP address or DNS. Ryansend is made for home users, so the best option is to set up DDNS, then forwarding a port in your router to point to whatever local IP ryansend is running on.

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
