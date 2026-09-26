#!/bin/sh

set -u

certificate_directory="${GEMINI_CERTIFICATE_DIRECTORY:-/var/lib/smallweb/certificates}"
mkdir -p "$certificate_directory"
chown -R nginx:nginx "$certificate_directory"

/usr/local/bin/generate-buildtime-badge

/docker-entrypoint.sh nginx -g "daemon off;" &
nginx_pid=$!

su-exec nginx:nginx /usr/local/bin/smallweb &
smallweb_pid=$!

terminate()
{
  trap - INT TERM
  kill -TERM "$nginx_pid" "$smallweb_pid" 2>/dev/null
  wait "$nginx_pid" 2>/dev/null
  wait "$smallweb_pid" 2>/dev/null
  exit 0
}

trap terminate INT TERM

while kill -0 "$nginx_pid" 2>/dev/null && kill -0 "$smallweb_pid" 2>/dev/null
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
  wait "$smallweb_pid"
  status=$?
fi

kill -TERM "$nginx_pid" "$smallweb_pid" 2>/dev/null
wait "$nginx_pid" 2>/dev/null
wait "$smallweb_pid" 2>/dev/null
exit "$status"
