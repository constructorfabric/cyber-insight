"""Manifest invariants for the GitLab streams.

The declarative manifest is checked here for the properties the mock-server
suite cannot see: every requester carries its own error handler (the CDK
default makes one project's 403 fatal to the stream and, on a fan-out parent,
silently truncates the roster), no hoisted column is deleted by a later
RemoveFields, every dated stream floors on the operator's start date, and the
spec refuses the instance-wide mode on gitlab.com.
"""

from __future__ import annotations

import jsonschema
import pytest
from config import GitlabConfigBuilder
from connector_tests.source import load_manifest

_CONNECTOR = "git/gitlab"


def _streams() -> list[dict]:
    return load_manifest(_CONNECTOR)["streams"]


def _requesters(node, out=None):
    if out is None:
        out = []
    if isinstance(node, dict):
        if node.get("type") == "HttpRequester":
            out.append(node)
        for value in node.values():
            _requesters(value, out)
    elif isinstance(node, list):
        for item in node:
            _requesters(item, out)
    return out


def _cursors(node, owner=None, out=None):
    if out is None:
        out = []
    if isinstance(node, dict):
        name = node.get("name") if isinstance(node.get("name"), str) else owner
        if node.get("type") == "DatetimeBasedCursor":
            out.append((name, node))
        for value in node.values():
            _cursors(value, name, out)
    elif isinstance(node, list):
        for item in node:
            _cursors(item, owner, out)
    return out


def test_every_requester_declares_an_error_handler() -> None:
    bare = [r.get("path", "<no path>") for r in _requesters(_streams()) if "error_handler" not in r]
    assert not bare, f"requesters relying on the CDK default error handler: {bare}"


def test_no_hoisted_field_is_deleted_by_a_later_remove() -> None:
    clashes: dict[str, list[str]] = {}
    for stream in _streams():
        added: dict[str, int] = {}
        removed: dict[str, int] = {}
        for index, transform in enumerate(stream.get("transformations", [])):
            if transform["type"] == "AddFields":
                for field in transform["fields"]:
                    added.setdefault(field["path"][0], index)
            if transform["type"] == "RemoveFields":
                for pointer in transform["field_pointers"]:
                    removed.setdefault(pointer[0], index)
        collided = sorted(n for n, at in added.items() if n in removed and removed[n] > at)
        if collided:
            clashes[stream["name"]] = collided
    assert not clashes, f"RemoveFields deletes a value AddFields just wrote: {clashes}"


def test_every_dated_stream_floors_on_the_start_date() -> None:
    """One bound in one direction: everything since the start date, nothing
    before. A rolling window or an epoch here either drops data inside the
    range the operator asked for or fetches outside it."""
    cursors = _cursors(_streams())
    assert cursors, "no incremental stream found — the audit is not looking at anything"
    off_floor = sorted(
        name for name, cursor in cursors if cursor["start_datetime"]["datetime"] != "{{ config['gitlab_start_date'] }}"
    )
    assert not off_floor, f"streams whose floor is not the configured start date: {off_floor}"


def test_every_stream_stamps_tenant_source_and_data_source() -> None:
    missing = []
    for stream in _streams():
        stamped = {
            field["path"][0]
            for transform in stream["transformations"]
            if transform["type"] == "AddFields"
            for field in transform["fields"]
        }
        if not {"tenant_id", "source_id", "data_source", "unique_key"} <= stamped:
            missing.append(stream["name"])
    assert not missing, missing


def test_every_stream_opts_out_of_schema_auto_import() -> None:
    flags = load_manifest(_CONNECTOR)["metadata"]["autoImportSchema"]
    names = {s["name"] for s in _streams()}
    assert set(flags) == names and not any(flags.values())


def test_the_proxy_never_learns_the_vendor_token_through_a_query_string() -> None:
    """The GitLab token rides to the proxy in a header only."""
    for requester in _requesters(_streams()):
        if "git_proxy_url" not in requester["url_base"]:
            continue
        params = " ".join(str(v) for v in (requester.get("request_parameters") or {}).values())
        assert "gitlab_token" not in params, requester["path"]


def _spec_schema() -> dict:
    return load_manifest(_CONNECTOR)["spec"]["connection_specification"]


@pytest.mark.parametrize(
    ("gitlab_url", "groups", "projects", "accepted"),
    [
        ("https://gitlab.com", [], [], False),
        ("https://gitlab.com/", [], [], False),
        ("https://gitlab.com", ["acme"], [], True),
        ("https://gitlab.com", [], ["acme/app"], True),
        ("https://gitlab.example.com", [], [], True),
    ],
)
def test_instance_wide_mode_is_refused_on_gitlab_com(
    gitlab_url: str, groups: list[str], projects: list[str], accepted: bool
) -> None:
    config = (
        GitlabConfigBuilder()
        .with_field("gitlab_url", gitlab_url)
        .with_field("gitlab_groups", groups)
        .with_field("gitlab_projects", projects)
        .build()
    )
    errors = list(jsonschema.Draft7Validator(_spec_schema()).iter_errors(config))
    assert (not errors) == accepted, f"should {'accept' if accepted else 'reject'}: {config}"
