#!/bin/sh

set -eu

: "${SEMPRE_SERVER_URL:?set SEMPRE_SERVER_URL to the isolated server under test}"
base_url=${SEMPRE_SERVER_URL%/}
email=${SEMPRE_SMOKE_EMAIL:-qa.server-smoke-$(date +%s)-$$@sempre.test}
password=${SEMPRE_SMOKE_PASSWORD:-$(openssl rand -hex 16)}
scratch=$(mktemp -d)
cookie_file=$scratch/cookies
stable_url=
subscription_id=

cleanup() {
  if [ -n "$subscription_id" ]; then
    curl --silent --max-time 5 -b "$cookie_file" -X DELETE \
      "$base_url/api/v1/subscriptions/$subscription_id" >/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup EXIT

request_manifest() {
  curl --fail --silent --show-error --max-time 30 \
    "$base_url/api/v1/public/subscriptions/$stable_url?target=sing-box-v13"
}

fetch_artifact() {
  manifest=$1
  destination=$2
  artifact_url=$(printf '%s' "$manifest" | jq -er '.artifact.url')
  case "$artifact_url" in
    "$base_url"/*) ;;
    *) echo 'artifact URL must share the manifest origin' >&2; exit 1 ;;
  esac
  curl --fail --silent --show-error --max-time 30 "$artifact_url" -o "$destination"
  expected=$(printf '%s' "$manifest" | jq -er '.artifact.sha256')
  actual=$(openssl dgst -sha256 -r "$destination" | cut -d ' ' -f 1)
  test "$actual" = "$expected"
}

patch_subscription() {
  current=$1
  node_name=$2
  if [ -n "$node_name" ]; then
    servers=$(jq -nc --arg name "$node_name" '[{name:$name,type:"socks5",server:"127.0.0.1",port:1080}]')
    group=$(jq -nc --arg name "$node_name" '[{name:"Smoke Group",type:"select",proxies:[$name,"DIRECT"]}]')
    upstream=null
  else
    servers='[]'
    group='[]'
    upstream='"http://127.0.0.1:1/qa-unreachable"'
  fi
  body=$(jq -nc --arg servers "$servers" --arg group "$group" --argjson upstream "$upstream" \
    '{servers:$servers,group:$group,subscribeUrl:$upstream}')
  updated_at=$(printf '%s' "$current" | jq -er '.updatedAt')
  curl --fail --silent --show-error --max-time 30 -b "$cookie_file" \
    -H 'Content-Type: application/json' -H "If-Match: $updated_at" \
    -X PATCH -d "$body" "$base_url/api/v1/subscriptions/$subscription_id"
}

register_body=$(jq -nc --arg email "$email" --arg password "$password" \
  '{name:"QA Server Smoke",email:$email,password:$password}')
curl --fail --silent --show-error --max-time 30 -c "$cookie_file" \
  -H 'Content-Type: application/json' -d "$register_body" \
  "$base_url/api/v1/auth/register" -o "$scratch/register.json"
jq -e '.user.role == "user" or .user.role == "superadmin"' "$scratch/register.json" >/dev/null

servers_a='[{"name":"Smoke-A","type":"socks5","server":"127.0.0.1","port":1080}]'
group_a='[{"name":"Smoke Group","type":"select","proxies":["Smoke-A","DIRECT"]}]'
create_body=$(jq -nc --arg servers "$servers_a" --arg group "$group_a" '{
  remark:"QA Server Smoke",servers:$servers,group:$group,
  useSystemRuleList:false,ruleList:"{}",useSystemGroup:false,
  useSystemFilter:false,filter:"[]",useSystemCustomConfig:false,
  customConfig:"[]",useSystemDnsConfig:false,dnsConfig:""
}')
current=$(curl --fail --silent --show-error --max-time 30 -b "$cookie_file" \
  -H 'Content-Type: application/json' -d "$create_body" \
  "$base_url/api/v1/subscriptions")
subscription_id=$(printf '%s' "$current" | jq -er '.id')
stable_url=$(printf '%s' "$current" | jq -er '.url')

manifest_a=$(request_manifest)
printf '%s' "$manifest_a" | jq -e \
  '.schema == 1 and .service == "sempre" and .read_only == true and .target.format == "sing-box-v13" and .artifact.node_count == 1' >/dev/null
fetch_artifact "$manifest_a" "$scratch/a.json"
grep -q 'Smoke-A' "$scratch/a.json"

current=$(patch_subscription "$current" Smoke-B)
test "$(printf '%s' "$current" | jq -er '.url')" = "$stable_url"
manifest_b=$(request_manifest)
fetch_artifact "$manifest_b" "$scratch/b.json"
grep -q 'Smoke-B' "$scratch/b.json"
test "$(printf '%s' "$manifest_a" | jq -er '.artifact.sha256')" != \
  "$(printf '%s' "$manifest_b" | jq -er '.artifact.sha256')"
fetch_artifact "$manifest_a" "$scratch/a-after-b.json"
cmp "$scratch/a.json" "$scratch/a-after-b.json"

current=$(patch_subscription "$current" Smoke-A)
manifest_a_again=$(request_manifest)
test "$(printf '%s' "$manifest_a_again" | jq -er '.artifact.sha256')" = \
  "$(printf '%s' "$manifest_a" | jq -er '.artifact.sha256')"
current=$(patch_subscription "$current" '')
curl --fail --silent --show-error --max-time 30 -D "$scratch/stale-headers" \
  "$base_url/api/public/proxy/$stable_url/sing-box/13" -o "$scratch/stale.json"
grep -iq '^x-sempre-stale: true' "$scratch/stale-headers"
cmp "$scratch/a.json" "$scratch/stale.json"
fetch_artifact "$manifest_b" "$scratch/b-after-failure.json"
cmp "$scratch/b.json" "$scratch/b-after-failure.json"

printf 'server smoke passed: stable URL, manifest SHA, immutable artifacts, and latest successful fallback\n'
