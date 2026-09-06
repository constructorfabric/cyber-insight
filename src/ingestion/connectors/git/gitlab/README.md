# GitLab (git-cli-proxy) Connector

Declarative GitLab connector that extracts commit-level data through the
[git-cli-proxy](../../../../../docs/components/connectors/git/git-cli-proxy/specs/DESIGN.md)
instead of the vendor API, and everything git does not hold — projects, merge
requests and their review history, pipelines, deployments, members — through the
GitLab REST v4 and GraphQL APIs. Works against gitlab.com and self-managed
instances alike.

Auth: one access token, sent as `PRIVATE-TOKEN` to the API and forwarded per
request (never stored) to the proxy for the clone.

## Prerequisites

1. A GitLab access token with the `read_api` and `read_repository` scopes. A
   personal, group or project access token all work; a group or project token
   sees only its own scope, so the instance-wide mode and the `users` stream
   need a personal one.
2. A reachable git-cli-proxy deployment and its bearer token. In-cluster the
   umbrella composes both (`insight-git-cli-proxy-config`).

## K8s Secret

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: insight-gitlab-main
  labels:
    app.kubernetes.io/part-of: insight
  annotations:
    insight.cyberfabric.com/connector: gitlab
    insight.cyberfabric.com/source-id: gitlab-main
type: Opaque
stringData:
  gitlab_url: "https://gitlab.example.com"
  gitlab_token: "CHANGE_ME"
  gitlab_groups: '["acme"]'
  gitlab_start_date: "2026-01-01"
```

The proxy's address and bearer token are absent by design: the chart owns both
and reconcile injects them.

`gitlab_start_date` is the one bound: no stream fetches anything older, and a
project nobody has touched since it is never cloned.

### Fields

| Field | Required | Description |
|-------|----------|-------------|
| `gitlab_url` | Yes | Base URL of the instance, e.g. `https://gitlab.com` or `https://gitlab.example.com` |
| `gitlab_token` | Yes | Access token with `read_api` and `read_repository` |
| `gitlab_groups` | No | JSON array of group full paths (`acme`, `acme/platform`); every project in the group and its subgroups is included |
| `gitlab_projects` | No | JSON array of project full paths for projects outside the groups |
| `gitlab_include_forks` | No | `"true"` to sync forked projects. Off by default: a fork's history is its upstream's, and counting it doubles the upstream's commits |
| `gitlab_exclude_projects` | No | JSON array of regular expressions matched (`search`) against `path_with_namespace`; a match is never listed, cloned or walked. Archived projects are always skipped |
| `gitlab_instance_users` | No | `"true"` to also sync the instance user directory (`/users`). Needs a personal token; `email` is returned only to an administrator. Leave off on gitlab.com and shared instances |
| `gitlab_start_date` | Yes | Earliest date fetched, by every stream (YYYY-MM-DD); bounds the first-sync cost |

Leaving both `gitlab_groups` and `gitlab_projects` empty syncs every project
the token can see (the instance-wide mode). The spec refuses that on
gitlab.com, where it would mean every public project on the platform.

### Automatically injected

| Field | Source |
|-------|--------|
| `insight_tenant_id` | `tenant_id` from tenant YAML |
| `insight_source_id` | `insight.cyberfabric.com/source-id` annotation |
| `git_proxy_url`, `git_proxy_token` | the chart's `insight-git-cli-proxy-config` |

### Local development

```bash
cp src/ingestion/secrets/connectors/gitlab.yaml.example src/ingestion/secrets/connectors/gitlab.yaml
# fill in real values, then:
kubectl apply -f src/ingestion/secrets/connectors/gitlab.yaml
```

## Upgrading from 3.x

The 3.x connector was a Python CDK source with its own stream shapes; 4.0
shares three stream names with it (`commits`, `branches`, `users`) under
different columns and different keys, and reconcile recreates the connection
on a definition change without touching the warehouse. Drop the connector's
bronze tables before the first 4.0 sync:

```sql
DROP DATABASE bronze_gitlab SYNC;
```

The next deploy recreates the placeholder and the first sync fills it from
`gitlab_start_date`. Staging models are rebuilt by dbt from the new bronze;
silver and gold need nothing.

## Streams

| Stream | Upstream | Sync Mode | Cursor |
|--------|----------|-----------|--------|
| `repositories` | GitLab projects listing per scope | incremental | `last_activity_at` |
| `commits` | proxy `/v1/commits` | incremental, per project | `committed_date` |
| `file_changes` | proxy `/v1/file-changes` | incremental, per project | `committed_date` |
| `branches` | proxy `/v1/branches` | full refresh, per project | — |
| `commit_authors` | proxy `/v1/authors`, then GitLab `/users?search=` | incremental, per project | `last_committed_date` |
| `pull_requests` | GitLab merge requests per scope, all states | incremental, monthly steps | `updated_at` |
| `pull_request_diff_stats` | GraphQL `mergeRequests.diffStatsSummary` | incremental, per project | `updatedAt` |
| `pull_request_notes` | `/merge_requests/{iid}/notes` | windowed MR parent | `updated_at` |
| `pull_request_commits` | `/merge_requests/{iid}/commits` | windowed MR parent | parent `updated_at` |
| `pull_request_state_events` | `/merge_requests/{iid}/resource_state_events` | windowed MR parent | `created_at` |
| `pull_request_label_events` | `/merge_requests/{iid}/resource_label_events` | windowed MR parent | `created_at` |
| `pipelines` | GraphQL `project.pipelines` | incremental, per project | `updatedAt` |
| `deployments` | `/projects/{id}/deployments` | incremental, per project | `updated_at` |
| `group_members` | `/groups/{g}/members/all`, `/projects/{p}/members/all` | full refresh, per configured scope | — |
| `users` | `/users` (keyset) | full refresh, opt-in | — |

### Scopes

Every vendor stream partitions over the configured scopes: each group (with
its subgroups), each project, or the whole instance when nothing is
configured. A scope decides the endpoint family — `/groups/{g}/projects`,
`/projects/{p}` or the instance-wide `/projects` — through one shared
expression, so the streams stay uniform across the three modes. The
instance-wide listing uses keyset pagination, which has no offset ceiling.

### How the streams fit together

`repositories` is the roster: it applies the fork and exclusion filters and is
the incremental **parent** of every proxy stream (`SubstreamPartitionRouter`
with `incremental_dependency: true`), so a sync clones only projects whose
`last_activity_at` advanced. The proxy routes on the project's
`http_url_to_repo`; bronze keys every proxy row on the numeric project id, so
forks sharing commit SHAs never collapse into one row and a rename changes
nothing.

Merge requests are listed once per scope, not once per project: a group
listing covers every project in the group in one paged walk. The children
(notes, commits, state and label events) fan out from a windowed copy of that
listing and persist its cursor, so a later sync visits only merge requests
updated since. A merge request from a project the roster excluded still
appears in the listing; the staging models join on the roster and drop it
there, so exclusion stays consistent.

`commit_authors` is the author bridge. The proxy enumerates each project's
distinct author e-mails from the clone it already holds, and one
`/users?search=<e-mail>` call per author names the account whose `email` or
`public_email` is exactly that address. That is what lets a commit reach a
person on an instance where the token cannot read the user directory.

### Where the review history comes from

GitLab records approvals, their withdrawal and requests for changes as system
notes on the merge request, each with the actor and the instant. The notes
stream therefore carries the whole review history; the approvals endpoint
(current approvers only, no timestamps, `402` on unlicensed editions) is not
used.

### GraphQL

REST exposes no line counts on a merge request — `changes_count` is a capped
string — and no source, queue time or merge-request link on a pipeline
without a second call each. Both come from GraphQL, one paged query per
project filtered server-side by `updatedAfter`. A GraphQL error arrives as
HTTP 200 with an `errors` array and fails the stream with GitLab's own
message; a `project: null` answer (the token can list a project it cannot
query) yields no rows and no error.

### Error handling

Three handlers, by what a status means at that endpoint:

- **scope discovery** (`/groups/{g}/projects`, `/projects/{p}`, members): a
  `403`/`404` is a wrong path or a token without membership — a
  configuration error, failed loudly.
- **per-project and per-merge-request endpoints**: a `402`/`403`/`404` is
  about one project (a feature not licensed, a restricted project, a deleted
  merge request) and skips it, never the stream.
- **proxy**: `429` + `Retry-After` while a clone runs is waited out (up to
  the CDK's ceiling), `404`/`413` skip the project, `409` (superseded
  snapshot) fails the attempt so the next one starts fresh.

`429` from GitLab itself is a rate limit and backs off on `Retry-After`.

## Silver Targets

The staging models under `dbt/` carry `silver:class_git_*` tags and feed the
shared git classes. `union_by_tag` UNION ALLs every tagged branch positionally,
so each model's SELECT list matches the class column-for-column and
type-for-type.

| Stream | Staging model | Class |
|---|---|---|
| `repositories` | `gitlab__repositories` | `class_git_repositories` |
| `branches` | `gitlab__repository_branches` | `class_git_repository_branches` |
| `commits` | `gitlab__commits` | `class_git_commits` |
| `file_changes` | `gitlab__file_changes` | `class_git_file_changes` |
| `pull_requests` + `pull_request_diff_stats` | `gitlab__pull_requests` | `class_git_pull_requests` |
| `pull_requests` (reviewers) + `pull_request_notes` (verdicts) | `gitlab__pull_requests_reviewers` | `class_git_pull_requests_reviewers` |
| `pull_request_notes` (user notes) | `gitlab__pull_requests_comments` | `class_git_pull_requests_comments` |
| `pull_request_commits` | `gitlab__pull_requests_commits` | `class_git_pull_requests_commits` |
| `pull_request_notes` | `gitlab__pr_review_events` | `class_git_pr_review_events` |
| `pull_request_state_events` + `pull_request_label_events` | `gitlab__item_events` | `class_git_item_events` |
| `pipelines` | `gitlab__ci_runs` | `class_git_ci_runs` |
| `deployments` | `gitlab__deployments`, `gitlab__deployment_events` | `class_git_deployments`, `class_git_deployment_events` |

Identity: `gitlab__account_emails` collects every (account, e-mail) pair from
the author lookup, the user directory, the rosters and the
`{id}-{username}@users.noreply.<host>` address form; `gitlab__account_names`
the display name per account; `gitlab__unowned_commit_emails` the addresses no
account claims. `gitlab__identity_inputs` publishes them into
`silver.identity_inputs`, keyed on the numeric user id — the same key
`gitlab__pull_requests` puts on a merge request's author.

## Tests

```bash
cd src/ingestion/tests/connectors
uv run pytest ../../connectors/git/gitlab/tests -q
```
