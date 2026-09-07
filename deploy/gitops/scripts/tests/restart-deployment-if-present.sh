#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
helper="$script_dir/../restart-deployment-if-present.sh"
tmp_dir="$(mktemp -d)"
calls="$tmp_dir/calls"
trap 'rm -rf "$tmp_dir"' EXIT

cat >"$tmp_dir/kubectl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$KUBECTL_CALLS"
if [[ "$*" == *" get deployment "* ]]; then
  if [[ "$KUBECTL_DEPLOYMENT_EXISTS" == "true" ]]; then
    printf 'deployment.apps/synthetic-v3-core\n'
  fi
  exit 0
fi
EOF
chmod +x "$tmp_dir/kubectl"

KUBECTL_CALLS="$calls" KUBECTL_DEPLOYMENT_EXISTS=false PATH="$tmp_dir:$PATH" \
  "$helper" synthetic-namespace synthetic-v3-core
[[ "$(wc -l <"$calls" | tr -d ' ')" == "1" ]]
[[ "$(cat "$calls")" == "-n synthetic-namespace get deployment synthetic-v3-core --ignore-not-found -o name" ]]

: >"$calls"
KUBECTL_CALLS="$calls" KUBECTL_DEPLOYMENT_EXISTS=true PATH="$tmp_dir:$PATH" \
  "$helper" synthetic-namespace synthetic-v3-core
[[ "$(wc -l <"$calls" | tr -d ' ')" == "2" ]]
grep -Fx -- "-n synthetic-namespace get deployment synthetic-v3-core --ignore-not-found -o name" "$calls" >/dev/null
grep -Fx -- "-n synthetic-namespace rollout restart deployment.apps/synthetic-v3-core" "$calls" >/dev/null
