#!/bin/sh

set -eu

base_url=${1:-http://127.0.0.1:8787}
email=${SEMPRE_SMOKE_EMAIL:-server-smoke@example.com}
password=${SEMPRE_SMOKE_PASSWORD:-server-smoke-password}

session_payload=$(curl --fail --silent \
  -H 'Content-Type: application/json' \
  -d "$(jq -n --arg email "$email" --arg password "$password" '{email:$email,password:$password}')" \
  "$base_url/api/v1/auth/register")
session_token=$(printf '%s' "$session_payload" | jq -er '.token')

profile_document=$(jq -n '{
  name: "Published",
  sources: [{
    id: "raw-source",
    type: "raw",
    enabled: true,
    content: "proxies:\n  - name: edge\n    type: socks5\n    server: 1.1.1.1\n    port: 1080"
  }]
}')
create_body=$(jq -n --argjson document "$profile_document" '{name:"Published",document:$document}')
profile_payload=$(curl --fail --silent \
  -H "Authorization: Bearer $session_token" \
  -H 'Content-Type: application/json' \
  -d "$create_body" \
  "$base_url/api/v1/profiles")
profile_id=$(printf '%s' "$profile_payload" | jq -er '.id')

target=$(curl --fail --silent "$base_url/api/v1/targets" \
  | jq -ec 'map(select(.format == "sing-box-v13" and .platform == "default"))[0]')
compile_body=$(jq -n --argjson target "$target" '{target:$target}')
compile_payload=$(curl --fail --silent \
  -H "Authorization: Bearer $session_token" \
  -H 'Content-Type: application/json' \
  -d "$compile_body" \
  "$base_url/api/v1/profiles/$profile_id/compile")
artifact_hash=$(printf '%s' "$compile_payload" | jq -er '.artifact_hash')

share_payload=$(curl --fail --silent \
  -H "Authorization: Bearer $session_token" \
  -H 'Content-Type: application/json' \
  -d '{}' \
  "$base_url/api/v1/profiles/$profile_id/shares")
manifest_url=$(printf '%s' "$share_payload" | jq -er '.url')
manifest_url="${manifest_url}?target=sing-box-v13"
curl --fail --silent "$manifest_url" \
  | jq -e --arg hash "$artifact_hash" \
    '.profile.revision == 1 and .profile.name == "Published" and .artifact.sha256 == $hash' \
    >/dev/null

draft_document=$(printf '%s' "$profile_payload" \
  | jq -c '.document | .name = "Draft" | .sources[0].content = "proxies:\n  - name: invalid-draft"')
update_body=$(jq -n --argjson document "$draft_document" '{name:"Draft",document:$document}')
curl --fail --silent -X PUT \
  -H "Authorization: Bearer $session_token" \
  -H 'Content-Type: application/json' \
  -H 'If-Match: "1"' \
  -d "$update_body" \
  "$base_url/api/v1/profiles/$profile_id" \
  | jq -e '.revision == 2 and .name == "Draft"' >/dev/null

curl --fail --silent "$manifest_url" \
  | jq -e --arg hash "$artifact_hash" \
    '.profile.revision == 1 and .profile.name == "Published" and .artifact.sha256 == $hash' \
    >/dev/null

printf 'server smoke passed: published revision 1 survived draft revision 2\n'
