# smolhost

`smolhost` is the runtime base image for
[hans.gerwitz.com](https://hans.gerwitz.com/). It combines Nginx with a Rust
service that exposes one generated Gemtext tree over Gemini and Spartan.

The image is published as `ghcr.io/gerwitz/smolhost`.
Published tags contain both `linux/amd64` and `linux/arm64` images.

## Runtime layout

- Nginx serves HTTP content from `/usr/share/nginx/html`.
- Gemini and Spartan serve Gemtext from `/srv/smallweb`.
- Requests under `/media/*` are fetched from the configured HTTPS media origin.
- Gemini certificates are stored in `/var/lib/smallweb/certificates`.
- The current content version's first deployment time is exposed as a status badge at
  `/.well-known/smolhost/buildtime.svg`.

The certificate directory must be persisted across deployments. Gemini clients
trust self-signed certificates across visits, so replacing a certificate causes
warnings.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `SMALLWEB_HOSTNAME` | `hans.gerwitz.com` | Host accepted by Gemini and Spartan |
| `SMALLWEB_ROOT` | `/srv/smallweb` | Generated Gemtext content |
| `GEMINI_LISTEN` | `0.0.0.0:1965` | Gemini listener |
| `GEMINI_CERTIFICATE_DIRECTORY` | `/var/lib/smallweb/certificates` | Persistent Gemini certificates |
| `SPARTAN_LISTEN` | `0.0.0.0:3000` | Spartan listener |
| `MEDIA_ORIGIN_HOST` | unset | HTTPS host used for `/media/*` |
| `SMOLHOST_CONTENT_ROOT` | `/usr/share/nginx/html` | Content tree fingerprinted for the badge |
| `SMOLHOST_BUILD_TIME` | current UTC time | Optional first-seen time in `YYYY-MM-DD HH:MM` format |

Spartan uploads are intentionally rejected. Static resources and media are
read-only on both protocols. Media responses are limited to 64 MiB.

The badge follows the conventional 20-pixel README badge format. It reads
`Updated YYYY-MM-DD HH:MM`, with a white-on-black label and black-on-white UTC
timestamp. Before Nginx starts, `smolhost` hashes every file in the content
tree and looks up the digest in a registry stored beside the persistent Gemini
certificate. New content receives the current time; restarts and rollbacks
reuse the digest's original timestamp. Badge generation is atomic.

## Derived images

A derived site image supplies its Nginx template and generated content:

```dockerfile
FROM ghcr.io/gerwitz/smolhost:latest

COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY _site /usr/share/nginx/html
COPY _site/editions/gemini /srv/smallweb
```

The container exposes HTTP on `80`, Gemini on `1965`, and Spartan on `3000`.
Map public Spartan port `300` to container port `3000`.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo run
```
