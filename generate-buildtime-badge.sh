#!/bin/sh

set -eu

certificate_directory="${GEMINI_CERTIFICATE_DIRECTORY:-/var/lib/smallweb/certificates}"
content_root="${SMOLHOST_CONTENT_ROOT:-/usr/share/nginx/html}"
output="${SMOLHOST_BUILD_BADGE_PATH:-/usr/share/nginx/html/.well-known/smolhost/buildtime.svg}"
state_file="${SMOLHOST_BUILD_STATE:-${certificate_directory}/content-builds}"

if [ ! -d "$content_root" ]
then
  echo "SMOLHOST_CONTENT_ROOT is not a directory: $content_root" >&2
  exit 1
fi

fingerprint=$(
  find "$content_root" -type f ! -path "$output" -exec sha256sum {} + |
    LC_ALL=C sort |
    sha256sum |
    awk '{print $1}'
)

mkdir -p "$(dirname "$state_file")"
touch "$state_file"

build_time=$(awk -F '|' -v fingerprint="$fingerprint" '
  $1 == fingerprint {
    print $2
    exit
  }
' "$state_file")

if [ -z "$build_time" ]
then
  build_time="${SMOLHOST_BUILD_TIME:-$(date -u "+%Y-%m-%d %H:%M")}"
  printf '%s|%s\n' "$fingerprint" "$build_time" >> "$state_file"
fi

case "$build_time" in
  ????-??-??\ ??:??)
    ;;
  *)
    echo "SMOLHOST_BUILD_TIME must use YYYY-MM-DD HH:MM" >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$output")"
temporary="${output}.tmp"

cat > "$temporary" <<EOF
<svg xmlns="http://www.w3.org/2000/svg" width="182" height="20" role="img" aria-label="Updated ${build_time}">
  <title>Updated ${build_time} UTC</title>
  <clipPath id="round">
    <rect width="182" height="20" rx="3"/>
  </clipPath>
  <g clip-path="url(#round)">
    <rect width="182" height="20" fill="#fff"/>
    <rect width="60" height="20" fill="#000"/>
  </g>
  <rect x=".5" y=".5" width="181" height="19" rx="3" fill="none" stroke="#000"/>
  <g font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle">
    <text x="30" y="14" fill="#fff">Updated</text>
    <text x="121" y="14" fill="#000">${build_time}</text>
  </g>
</svg>
EOF

mv "$temporary" "$output"
