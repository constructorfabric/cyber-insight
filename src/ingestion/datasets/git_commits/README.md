# git_commits

One row per commit a person wrote.

## What one row is

A commit, once, per tenant and source system. The same commit reaching the warehouse from a
fork and its upstream is one row: the hash collapse keeps the first, and every row carries
the surviving commit's repository and project.

Merge commits are excluded. So are derived commits — a squash or rebase result that
re-applies work an earlier commit already carried — because counting both would count the
work twice.

## What the numbers mean

`lines_added` and `lines_removed` are the commit's own stats, less the lines of its file
changes that lost the content dedup. A commit that introduces nothing new therefore reports
zero, and its size agrees with the file changes attributed to it. Both are absent, not zero,
when the source reported no stats at all.

## Where it comes from

The shared commit stage, which resolves the person, the branch scope and the repository
dimensions; and the authored file-change relation, for the lines to deduct.

## Watch for

- `tenant_id` and `source_id` are nullable on the class relations and stay nullable here;
  `source_id` groups its absent rows under `__unknown__`.
- `authored_at` is the author date. A rebase copy and its original share it, which is why the
  dedup upstream breaks ties on the committer date.
