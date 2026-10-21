#!/bin/sh
# Run by the nginx image before nginx starts. Gets the https certificate ready and keeps it current:
#
#   /tls/generated/   made here (self-signed) for the names below, and made again when it is missing, when the names
#                     change, when it is within a month of its end, or when Settings asks for a new one
#   /tls/custom/      a certificate and key the owner uploaded in Settings (the backend writes them); served instead of
#                     the generated one while both files are there
#
# The names the generated certificate is for: those in TLS_NAMES (the start scripts pass this machine's addresses as they
# are when they run) and those the backend wrote to /tls/extra-names.txt (what Settings' "make a new one" found: this
# machine's addresses as they are now and the address the owner is using). /tls/regenerate asks for a new certificate
# even when nothing changed; it is taken away once the certificate is made.
#
# nginx reads /etc/nginx/tls-active/, which points at whichever is in use. A loop takes effect of an upload, a removal or
# a new certificate without a restart. Each tick of it is one `stat` of the files that matter, about two processes: the
# real work (the checks with openssl, nginx's reload) is done only when what `stat` says has changed, and once an hour for
# what no file shows (the certificate running out).
set -e

TLS=/tls
ACTIVE=/etc/nginx/tls-active
mkdir -p "$TLS/generated" "$TLS/custom" "$ACTIVE"

# The backend runs as the user who started the stack, not as root: the folder it writes the uploaded certificate to
# has to be theirs. (It is root's if Docker made the mounted folder itself; the start scripts make it beforehand.)
chown "$(stat -c '%u:%g' "$TLS")" "$TLS/custom" 2>/dev/null || true

# The page asks where https is, to offer a link to it (the port is the one published, not 443 as the container sees it)
printf '{"port": %s}\n' "${HTTPS_PORT:-443}" > /usr/share/nginx/html/tls.json

# Every name the generated certificate is to cover, sorted and without repeats
effective_names() {
  { echo "${TLS_NAMES:-localhost}" | tr ',' '\n'; cat "$TLS/extra-names.txt" 2>/dev/null; } \
    | tr -d '\r ' | sed '/^$/d' | sort -u | tr '\n' ',' | sed 's/,$//'
}

generate() {
  san="DNS:localhost,IP:127.0.0.1"
  for name in $(echo "$1" | tr ',' ' '); do
    # Already in the list above
    case "$name" in ""|localhost|127.0.0.1) continue ;; esac
    if echo "$name" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$|^[0-9a-fA-F:]+:[0-9a-fA-F:]*$'; then
      san="$san,IP:$name"
    else
      san="$san,DNS:$name"
    fi
  done
  echo "tls: making a self-signed certificate for $san"
  # 825 days: the longest a certificate may last on Apple's systems. Written beside its place and moved into it, so
  # nginx never reads half of a pair.
  openssl req -x509 -newkey rsa:2048 -nodes -days 825 \
    -keyout "$TLS/generated/key.pem.new" -out "$TLS/generated/cert.pem.new" \
    -subj "/CN=llm-tulpa" -addext "subjectAltName=$san" >/dev/null 2>&1
  mv "$TLS/generated/key.pem.new" "$TLS/generated/key.pem"
  mv "$TLS/generated/cert.pem.new" "$TLS/generated/cert.pem"
  echo "$1" > "$TLS/generated/names.txt"
}

# Makes the generated certificate when it is missing, stale, or asked for
ensure_generated() {
  wanted="$(effective_names)"
  if [ -f "$TLS/regenerate" ] \
     || [ ! -s "$TLS/generated/cert.pem" ] || [ ! -s "$TLS/generated/key.pem" ] \
     || [ "$(cat "$TLS/generated/names.txt" 2>/dev/null)" != "$wanted" ] \
     || ! openssl x509 -checkend 2592000 -noout -in "$TLS/generated/cert.pem" >/dev/null 2>&1; then
    generate "$wanted"
    rm -f "$TLS/regenerate"
  fi
}

# Whether the certificate and the key at these paths are one pair (nginx would refuse to start with two that are not)
is_pair() {
  [ "$(openssl x509 -noout -pubkey -in "$1" 2>/dev/null | openssl sha256 2>/dev/null)" = "$(openssl pkey -pubout -in "$2" 2>/dev/null | openssl sha256 2>/dev/null)" ] \
    && openssl x509 -noout -in "$1" >/dev/null 2>&1
}

# Which certificate nginx is to use now: the uploaded one when it is a usable pair, else the generated one
select_certificate() {
  if [ -s "$TLS/custom/cert.pem" ] && [ -s "$TLS/custom/key.pem" ] && is_pair "$TLS/custom/cert.pem" "$TLS/custom/key.pem"; then
    ln -sfn "$TLS/custom/cert.pem" "$ACTIVE/cert.pem"
    ln -sfn "$TLS/custom/key.pem" "$ACTIVE/key.pem"
    echo custom
  else
    ln -sfn "$TLS/generated/cert.pem" "$ACTIVE/cert.pem"
    ln -sfn "$TLS/generated/key.pem" "$ACTIVE/key.pem"
    echo generated
  fi
}

# What changes when anything that decides the certificate does: the files of both, the names, the request for a new one.
# A file that is missing is a line less, so a deleted certificate shows too.
signature() {
  stat -c '%n %Y %s' "$TLS/custom/cert.pem" "$TLS/custom/key.pem" "$TLS/generated/cert.pem" "$TLS/generated/key.pem" \
    "$TLS/extra-names.txt" "$TLS/regenerate" 2>/dev/null
}

ensure_generated
echo "tls: using the $(select_certificate) certificate"

# The loop's own sleep, in seconds, and how many ticks make an hour
TICK=5
HOUR_TICKS=720

(
  # A failed check or a failed reload is no reason for this loop to end
  set +e
  last="$(signature)"
  ticks=0
  while sleep "$TICK"; do
    ticks=$((ticks + 1))
    now="$(signature)"
    # Nothing changed, and not yet time for the hourly look at the certificate's end: nothing to do
    if [ "$now" = "$last" ] && [ "$ticks" -lt "$HOUR_TICKS" ]; then
      continue
    fi
    ticks=0
    ensure_generated
    which="$(select_certificate)"
    after="$(signature)"
    # nginx is reloaded for a change in the files, or for a certificate the checks just made; not for the hourly look that
    # found nothing to do
    if [ "$now" != "$last" ] || [ "$after" != "$now" ]; then
      if nginx -t >/dev/null 2>&1 && nginx -s reload >/dev/null 2>&1; then
        echo "tls: reloaded, using the $which certificate"
      else
        echo "tls: the new certificate was not accepted by nginx; staying as it was" >&2
      fi
    fi
    # What the checks themselves changed is not a change to act on again
    last="$after"
  done
) &
