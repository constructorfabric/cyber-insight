#!/usr/bin/env bash
set -euo pipefail

namespace="${1:?namespace is required}"
deployment="${2:?deployment is required}"

resource="$(kubectl -n "$namespace" get deployment "$deployment" --ignore-not-found -o name)"
[[ -n "$resource" ]] || exit 0

kubectl -n "$namespace" rollout restart "$resource" >/dev/null
