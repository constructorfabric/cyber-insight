# git_file_changes

One row per path a commit touched.

## What one row is

A tenant, a source, a commit, a file path and a normalized change type. Two content
identities at one path in one commit fold into a single row carrying the sum of their lines,
so a line total here always matches the metric evidence.

## What the numbers mean

`lines_added` and `lines_removed` are the lines of the surviving change. A change whose
content an earlier commit already carried — a cherry-pick, a squashed branch, a
reverted-then-restored file — is not here at all: it lost the content dedup, and counting it
would count those lines twice.

## Where it comes from

`git_authored_file_changes`, the shared dedup relation, joined to the surviving commit for
the person, the dates and the repository dimensions. Those are inherited, never recomputed:
lines belong to the bucket their commit belongs to.

## Watch for

- `file_path` and `commit_hash` are dimensions, so a question may group by them; the row and
  answer limits are what keep such a breakdown bounded.
- `file_extension` and `change_type` are lower-cased, and rows with neither report
  `__unknown__`.
- `category` is the shipped classification of a path. A caller who disagrees can filter
  `file_path` by their own pattern instead.
