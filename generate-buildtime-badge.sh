#!/bin/sh

set -eu

build_time="${SMOLHOST_BUILD_TIME:-$(date -u "+%Y-%m-%d %H:%M")}"
output="${SMOLHOST_BUILD_BADGE_PATH:-/usr/share/nginx/html/.well-known/smolhost/buildtime.svg}"

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
<svg xmlns="http://www.w3.org/2000/svg" width="167" height="20" role="img" aria-label="Built ${build_time}">
  <title>Built ${build_time} UTC</title>
  <clipPath id="round">
    <rect width="167" height="20" rx="3"/>
  </clipPath>
  <g clip-path="url(#round)">
    <rect width="167" height="20" fill="#fff"/>
    <rect width="45" height="20" fill="#000"/>
  </g>
  <rect x=".5" y=".5" width="166" height="19" rx="3" fill="none" stroke="#000"/>
  <g font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11" text-anchor="middle">
    <text x="22.5" y="14" fill="#fff">Built</text>
    <text x="106" y="14" fill="#000">${build_time}</text>
  </g>
</svg>
EOF

mv "$temporary" "$output"
