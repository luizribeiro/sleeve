#!/usr/bin/env bash
set -euo pipefail

repository=$(cd "$(dirname "$0")/.." && pwd)
sleeve="$repository/crates/sleeve-core/src/http_wrapper.rs"
middleware=${1:-"$repository/../wasm-component-middleware/crates/wasm-component-middleware-wasi-http/src/p3.rs"}
filesystem="$repository/crates/sleeve-core/src/filesystem_wrapper.rs"
middleware_filesystem=${2:-"$repository/../wasm-component-middleware/crates/wasm-component-middleware-wasi/src/p3/filesystem.rs"}

if [[ ! -f "$middleware" ]]; then
  echo "middleware p3 gate not found: $middleware" >&2
  exit 1
fi

if [[ ! -f "$middleware_filesystem" ]]; then
  echo "middleware p3 filesystem gate not found: $middleware_filesystem" >&2
  exit 1
fi

range_lines() {
  local file=$1
  local first=$2
  local next_marker=$3
  awk -v first="$first" -v next_marker="$next_marker" '
    index($0, first) { counting = 1 }
    counting && index($0, next_marker) { exit }
    counting { lines++ }
    END { print lines + 0 }
  ' "$file"
}

sleeve_fields=$(range_lines "$sleeve" 'GuestFields for WrappedFields' 'GuestRequestOptions for WrappedOptions')
sleeve_options=$(range_lines "$sleeve" 'GuestRequestOptions for WrappedOptions' 'GuestRequest for WrappedRequest')
sleeve_request=$(range_lines "$sleeve" 'GuestRequest for WrappedRequest' 'GuestResponse for WrappedResponse')
sleeve_response=$(range_lines "$sleeve" 'GuestResponse for WrappedResponse' 'wasi::http::client::Guest for Component')
sleeve_client=$(range_lines "$sleeve" 'wasi::http::client::Guest for Component' 'platform::lifecycle::Guest for Component')

middleware_fields=$(range_lines "$middleware" 'types::HostFields for Gate' 'types::HostRequest for Gate')
middleware_options=$(range_lines "$middleware" 'types::HostRequestOptions for Gate' 'types::HostResponse for Gate')
middleware_request=$((
  $(range_lines "$middleware" 'types::HostRequest for Gate' 'types::HostRequestOptions for Gate') +
  $(range_lines "$middleware" 'types::HostRequestWithStore<T> for GateData' 'types::HostResponseWithStore<T> for GateData')
))
middleware_response=$((
  $(range_lines "$middleware" 'types::HostResponse for Gate' 'types::Host for Gate') +
  $(range_lines "$middleware" 'types::HostResponseWithStore<T> for GateData' 'client::Host for Gate')
))
middleware_client=$(range_lines "$middleware" 'client::Host for Gate' '/// Adds Preview 3 HTTP interfaces')

printf '%-18s %8s %12s\n' 'surface' 'sleeve' 'middleware'
printf '%-18s %8d %12d\n' 'fields' "$sleeve_fields" "$middleware_fields"
printf '%-18s %8d %12d\n' 'request-options' "$sleeve_options" "$middleware_options"
printf '%-18s %8d %12d\n' 'request' "$sleeve_request" "$middleware_request"
printf '%-18s %8d %12d\n' 'response' "$sleeve_response" "$middleware_response"
printf '%-18s %8d %12d\n' 'client' "$sleeve_client" "$middleware_client"
printf '\n%-18s %8s %12s\n' 'filesystem' 'sleeve' 'middleware'
printf '%-18s %8d %12d\n' 'subset/full gate' "$(wc -l < "$filesystem")" "$(wc -l < "$middleware_filesystem")"
