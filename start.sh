#!/bin/sh

set -u

certificate_directory="${GEMINI_CERTIFICATE_DIRECTORY:-/var/lib/calmserve/certificates}"
mkdir -p "$certificate_directory"
chown -R nginx:nginx "$certificate_directory"

/usr/local/bin/generate-status-badge

/docker-entrypoint.sh nginx -g "daemon off;" &
nginx_pid=$!

su-exec nginx:nginx /usr/local/bin/calmserve &
calmserve_pid=$!

terminate()
{
  trap - INT TERM
  kill -TERM "$nginx_pid" "$calmserve_pid" 2>/dev/null
  wait "$nginx_pid" 2>/dev/null
  wait "$calmserve_pid" 2>/dev/null
  exit 0
}

trap terminate INT TERM

while kill -0 "$nginx_pid" 2>/dev/null && kill -0 "$calmserve_pid" 2>/dev/null
do
  sleep 1 &
  wait $!
done

status=0
if ! kill -0 "$nginx_pid" 2>/dev/null
then
  wait "$nginx_pid"
  status=$?
else
  wait "$calmserve_pid"
  status=$?
fi

kill -TERM "$nginx_pid" "$calmserve_pid" 2>/dev/null
wait "$nginx_pid" 2>/dev/null
wait "$calmserve_pid" 2>/dev/null
exit "$status"
