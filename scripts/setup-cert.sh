#!/usr/bin/env bash
# Create a self-signed code signing cert in the user's login keychain.
# Once OneTrueDutchie is signed with this cert, macOS TCC remembers permission
# grants across rebuilds (ad-hoc signing does not — the cdhash changes every
# rebuild and TCC re-prompts).
#
# Idempotent: skips if the cert already exists.
set -euo pipefail

CERT_NAME="OneTrueDutchie Local Dev"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -p codesigning -v "$KEYCHAIN" 2>/dev/null | grep -q "$CERT_NAME"; then
  echo "✓ code signing identity '$CERT_NAME' already exists"
  exit 0
fi

echo "▶ Creating self-signed code signing certificate '$CERT_NAME'..."

TMP=$(mktemp -d)
trap "rm -rf '$TMP'" EXIT

cat > "$TMP/openssl.cnf" <<EOF
[ req ]
default_md = sha256
distinguished_name = dn
prompt = no
x509_extensions = v3_codesign

[ dn ]
CN = $CERT_NAME

[ v3_codesign ]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -nodes -newkey rsa:2048 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -days 3650 -config "$TMP/openssl.cnf" 2>/dev/null

openssl pkcs12 -export -out "$TMP/cert.p12" \
  -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
  -password pass:onetrue \
  -name "$CERT_NAME" 2>/dev/null

# Import key + cert. -A allows any app to access the key (avoids GUI prompts
# during codesign). -T /usr/bin/codesign ensures codesign can use it.
security import "$TMP/cert.p12" -k "$KEYCHAIN" \
  -P onetrue -A -T /usr/bin/codesign 2>&1 | grep -v "already" || true

# Trust the cert for code signing in the user's domain (no sudo needed).
security add-trusted-cert -r trustRoot -p codeSign \
  -k "$KEYCHAIN" "$TMP/cert.pem" 2>/dev/null \
  || echo "  (trust step skipped — cert still usable for codesign)"

# Verify
if security find-identity -p codesigning -v "$KEYCHAIN" | grep -q "$CERT_NAME"; then
  echo "✓ certificate created and trusted for code signing"
else
  echo "⚠ certificate created but not trusted; codesign will warn but still sign"
fi
