# calmserve

`calmserve` is the runtime base image for
[hans.gerwitz.com](https://hans.gerwitz.com/). It combines Nginx with a Rust
service that exposes one generated Gemtext tree over Gemini and Spartan.

The image is published as `ghcr.io/gerwitz/calmserve`.
Published tags contain both `linux/amd64` and `linux/arm64` images.

## Runtime layout

- Nginx serves HTTP content from `/usr/share/nginx/html`.
- Gemini and Spartan serve Gemtext from `/srv/calmserve`.
- Requests under `/media/*` are fetched from the configured HTTPS media origin.
- Gemini certificates are stored in `/var/lib/calmserve/certificates`.
- The current content version's first deployment time is exposed as a status badge at
  `/.well-known/calmserve/updated.svg`.

The certificate directory must be persisted across deployments. Gemini clients
trust self-signed certificates across visits, so replacing a certificate causes
warnings.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `CALMSERVE_HOSTNAME` | `hans.gerwitz.com` | Host accepted by Gemini and Spartan |
| `CALMSERVE_ROOT` | `/srv/calmserve` | Generated Gemtext content |
| `GEMINI_LISTEN` | `0.0.0.0:1965` | Gemini listener |
| `GEMINI_CERTIFICATE_DIRECTORY` | `/var/lib/calmserve/certificates` | Persistent Gemini certificates |
| `SPARTAN_LISTEN` | `0.0.0.0:3000` | Spartan listener |
| `MEDIA_ORIGIN_HOST` | unset | HTTPS host used for `/media/*` |
| `CALMSERVE_CONTENT_ROOT` | `/usr/share/nginx/html` | Content tree fingerprinted for the badge |
| `CALMSERVE_UPDATE_TIME` | current UTC time | Optional first-seen time in `YYYY-MM-DD HH:MM` format |

Spartan uploads are intentionally rejected. Static resources and media are
read-only on both protocols. Media responses are limited to 64 MiB.

The badge follows the conventional 20-pixel README badge format. It reads
`Updated YYYY-MM-DD HH:MM UTC`, with a white-on-black label and black-on-white
timestamp. Before Nginx starts, `calmserve` hashes every file in the content
tree and looks up the digest in a registry stored beside the persistent Gemini
certificate. New content receives the current time; restarts and rollbacks
reuse the digest's original timestamp. Badge generation is atomic.

The former `SMALLWEB_HOSTNAME`, `SMALLWEB_ROOT`, and `SMOLHOST_*` environment
variables remain temporary compatibility aliases.

## Static redirects

The generated Gemtext tree can include an optional `redirects.json` at
`CALMSERVE_ROOT` (default `/srv/calmserve`). Its format is a JSON object mapping
old request paths to target URLs, with string keys and string values:

```json
{
  "/writing/": "/notes/",
  "/writing": "/notes/",
  "/writing/2005-01-02-old.gmi": "/notes/old/"
}
```

The site generator supplies this file alongside its Gemtext content. Gemini and
Spartan look up exact paths before serving static files or applying directory
redirects, even when the old file no longer exists. Include both trailing-slash
variants when both should redirect. Query strings do not affect lookup. Targets
are returned unchanged and may be relative URLs or absolute URLs.

The map is loaded at startup; restart the service after changing it. A missing
file disables configured redirects. An unreadable file or invalid JSON object
prevents startup with an error. Unmatched paths retain normal file serving,
path validation still applies, and `/media/*` continues to use the media proxy.
These redirects do not configure Nginx's HTTP behavior.

## Derived images

A derived site image supplies its Nginx template and generated content:

```dockerfile
FROM ghcr.io/gerwitz/calmserve:latest

COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY _site /usr/share/nginx/html
COPY _site/editions/gemini /srv/calmserve
```

The container exposes HTTP on `80`, Gemini on `1965`, and Spartan on `3000`.
Map public Spartan port `300` to container port `3000`.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```
