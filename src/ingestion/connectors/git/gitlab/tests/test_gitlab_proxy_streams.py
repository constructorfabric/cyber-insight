"""Mock-server tests for the git-cli-proxy streams (commits, file_changes,
branches, and the author enumeration under commit_authors).

The proxy contract under test: cursor pagination on next_page_token, 429 +
Retry-After while a clone runs (retry, then succeed), 404/413 skipping the
project without failing the sync, and the project roster that decides which
clones happen at all — forks and excluded paths never reach the proxy, a
project idle since the start date is not cloned.
"""

from __future__ import annotations

import json
from typing import Any

import freezegun
from config import API_URL, GITLAB_URL, PROXY_URL, GitlabConfigBuilder
from connector_tests import ANY_QUERY_PARAMS, HttpMocker, HttpRequest, HttpResponse, assert_records_conform, read_stream

_CONNECTOR = "git/gitlab"
_PROJECTS_URL = f"{API_URL}/groups/acme/projects"
_CLONE_URL = f"{GITLAB_URL}/acme/app.git"
_FROZEN = "2026-07-01T00:00:00Z"


def _project(**overrides: Any) -> dict[str, Any]:
    return {
        "id": 7,
        "path": "app",
        "path_with_namespace": "acme/app",
        "http_url_to_repo": _CLONE_URL,
        "default_branch": "main",
        "archived": False,
        "last_activity_at": "2026-06-20T10:00:00.000+00:00",
        **overrides,
    }


def _projects_page(*projects: dict[str, Any]) -> HttpResponse:
    return HttpResponse(body=json.dumps(list(projects or [_project()])), status_code=200)


def _commit(sha: str) -> dict[str, Any]:
    return {
        "sha": sha,
        "message": f"commit {sha}",
        "authored_date": "2026-06-15T10:00:00Z",
        "committed_date": "2026-06-15T10:00:00Z",
        "author_name": "Dev",
        "author_email": "dev@example.com",
        "committer_name": "Dev",
        "committer_email": "dev@example.com",
        "parent_hashes": [],
        "is_merge": False,
        "is_in_default_branch": True,
        "patch_id": None,
    }


def _page(items: list[dict[str, Any]], next_token: str | None = None) -> HttpResponse:
    return HttpResponse(body=json.dumps({"items": items, "next_page_token": next_token}), status_code=200)


def _proxy_calls(http_mocker: HttpMocker, endpoint: str) -> list[str]:
    return [r.url for r in http_mocker._mocker.request_history if f"/v1/{endpoint}" in r.url]


@freezegun.freeze_time(_FROZEN)
def test_commits_paginate_and_key_on_the_project_id(http_mocker: HttpMocker) -> None:
    """Forks share SHAs, so the project is part of the key — by numeric id, which
    survives a rename, not by path."""
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/commits", query_params=ANY_QUERY_PARAMS),
        [_page([_commit("a" * 40)], next_token="t1"), _page([_commit("b" * 40)])],
    )

    output = read_stream(_CONNECTOR, "commits", config)

    assert not output.errors
    assert len(output.records) == 2
    rec = output.records[0].record.data
    assert rec["tenant_id"] == config["insight_tenant_id"]
    assert rec["source_id"] == config["insight_source_id"]
    assert rec["data_source"] == "insight_gitlab"
    assert rec["project_id"] == 7
    assert rec["repo_path"] == "acme/app"
    assert rec["repository"] == _CLONE_URL
    assert rec["unique_key"] == f"test-tenant:test-source:7:{'a' * 40}"
    calls = _proxy_calls(http_mocker, "commits")
    assert "page_token=t1" in calls[1]
    assert "since=2026-06-01" in calls[0], "the start date floors the walk"
    assert_records_conform(output.records, _CONNECTOR, "commits", strict=True)


@freezegun.freeze_time(_FROZEN)
def test_429_while_the_clone_runs_is_a_wait_not_a_failure(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/commits", query_params=ANY_QUERY_PARAMS),
        [HttpResponse(body="", status_code=429, headers={"Retry-After": "0"}), _page([_commit("c" * 40)])],
    )

    output = read_stream(_CONNECTOR, "commits", config)

    assert not output.errors
    assert len(output.records) == 1


@freezegun.freeze_time(_FROZEN)
def test_a_project_gone_at_origin_is_skipped_not_fatal(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/commits", query_params=ANY_QUERY_PARAMS), HttpResponse(body="", status_code=404)
    )

    output = read_stream(_CONNECTOR, "commits", config)

    assert not output.errors
    assert output.records == []


@freezegun.freeze_time(_FROZEN)
def test_file_changes_never_ask_for_the_patch(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/file-changes", query_params=ANY_QUERY_PARAMS),
        _page(
            [
                {
                    "sha": "a" * 40,
                    "committed_date": "2026-06-15T10:00:00Z",
                    "filename": "src/main.rs",
                    "status": "modified",
                    "additions": 3,
                    "deletions": 1,
                    "pre_image_oid": "1" * 40,
                    "post_image_oid": "2" * 40,
                }
            ]
        ),
    )

    output = read_stream(_CONNECTOR, "file_changes", config)

    assert not output.errors
    rec = output.records[0].record.data
    assert rec["unique_key"] == f"test-tenant:test-source:7:{'a' * 40}:src/main.rs"
    assert "include_patch=false" in _proxy_calls(http_mocker, "file-changes")[0]
    assert_records_conform(output.records, _CONNECTOR, "file_changes", strict=True)


@freezegun.freeze_time(_FROZEN)
def test_branches_key_on_project_and_name(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/branches", query_params=ANY_QUERY_PARAMS),
        _page(
            [{"name": "main", "head_sha": "f" * 40, "head_committed_date": "2026-06-20T10:00:00Z", "is_default": True}]
        ),
    )

    output = read_stream(_CONNECTOR, "branches", config)

    assert not output.errors
    rec = output.records[0].record.data
    assert rec["unique_key"] == "test-tenant:test-source:7:main"
    assert rec["is_default"] is True
    assert_records_conform(output.records, _CONNECTOR, "branches", strict=True)


@freezegun.freeze_time(_FROZEN)
def test_roster_admits_neither_forks_nor_excluded_paths_nor_idle_projects(http_mocker: HttpMocker) -> None:
    """A fork carries its upstream's whole history, an excluded path is the
    operator's call, and a project idle since the start date holds nothing
    inside the window: none of the three is worth a clone."""
    config = GitlabConfigBuilder().with_field("gitlab_exclude_projects", ["^acme/sandbox/"]).build()
    fork = _project(
        id=8,
        path_with_namespace="acme/app-fork",
        http_url_to_repo=f"{GITLAB_URL}/acme/app-fork.git",
        forked_from_project={"id": 7, "path_with_namespace": "acme/app"},
    )
    excluded = _project(
        id=9, path_with_namespace="acme/sandbox/scratch", http_url_to_repo=f"{GITLAB_URL}/acme/sandbox/scratch.git"
    )
    idle = _project(
        id=10,
        path_with_namespace="acme/dormant",
        http_url_to_repo=f"{GITLAB_URL}/acme/dormant.git",
        last_activity_at="2024-01-01T00:00:00.000+00:00",
    )
    http_mocker.get(
        HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page(_project(), fork, excluded, idle)
    )
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/branches", query_params=ANY_QUERY_PARAMS),
        _page(
            [{"name": "main", "head_sha": "f" * 40, "head_committed_date": "2026-06-20T10:00:00Z", "is_default": True}]
        ),
    )

    output = read_stream(_CONNECTOR, "branches", config)

    assert not output.errors
    walked = {r.record.data["repository"] for r in output.records}
    assert walked == {_CLONE_URL}, walked


@freezegun.freeze_time(_FROZEN)
def test_forks_are_admitted_when_the_operator_asks(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().with_field("gitlab_include_forks", True).build()
    fork = _project(
        id=8,
        path_with_namespace="acme/app-fork",
        http_url_to_repo=f"{GITLAB_URL}/acme/app-fork.git",
        forked_from_project={"id": 7, "path_with_namespace": "acme/app"},
    )
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page(_project(), fork))
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/branches", query_params=ANY_QUERY_PARAMS),
        _page(
            [{"name": "main", "head_sha": "f" * 40, "head_committed_date": "2026-06-20T10:00:00Z", "is_default": True}]
        ),
    )

    output = read_stream(_CONNECTOR, "branches", config)

    assert {r.record.data["project_id"] for r in output.records} == {7, 8}


def _authors_page(*rows: dict[str, Any]) -> HttpResponse:
    return HttpResponse(body=json.dumps({"items": list(rows), "next_page_token": None}), status_code=200)


def _author(email: str, sha: str) -> dict[str, Any]:
    return {
        "author_email": email,
        "author_name": "Ada",
        "sample_sha": sha,
        "last_committed_date": "2026-06-15T10:00:00+00:00",
        "commit_count": 3,
    }


def _user(uid: int, username: str, **fields: Any) -> dict[str, Any]:
    return {
        "id": uid,
        "username": username,
        "name": username.title(),
        "state": "active",
        "avatar_url": "x",
        "web_url": "y",
        **fields,
    }


@freezegun.freeze_time(_FROZEN)
def test_commit_authors_claim_an_account_only_on_an_exact_address_match(http_mocker: HttpMocker) -> None:
    """`/users?search=` matches on name and username too; a hit whose address
    is not the git ident claims nothing. Case differs between a git ident and
    a profile, so the comparison folds it."""
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/authors", query_params=ANY_QUERY_PARAMS),
        _authors_page(_author("Ada@example.com", "a" * 40)),
    )
    http_mocker.get(
        HttpRequest(f"{API_URL}/users", query_params=ANY_QUERY_PARAMS),
        HttpResponse(
            body=json.dumps(
                [
                    _user(41, "ada.other", public_email="ada.other@example.com"),
                    _user(42, "ada", public_email="ada@example.com"),
                ]
            ),
            status_code=200,
        ),
    )

    output = read_stream(_CONNECTOR, "commit_authors", config)

    assert not output.errors
    assert len(output.records) == 1
    rec = output.records[0].record.data
    assert rec["author_email"] == "Ada@example.com", "keyed on the git ident, not the profile"
    assert (rec["author_account_id"], rec["author_username"], rec["matched_field"]) == (42, "ada", "public_email")
    assert rec["project_id"] == 7
    assert rec["sample_sha"] == "a" * 40
    assert rec["unique_key"] == "test-tenant:test-source:7:ada@example.com", (
        "the key folds case; the column keeps the ident"
    )
    assert "avatar_url" not in rec
    lookup = next(r.url for r in http_mocker._mocker.request_history if "/users" in r.url)
    assert "search=Ada%40example.com" in lookup
    assert_records_conform(output.records, _CONNECTOR, "commit_authors", strict=True)


@freezegun.freeze_time(_FROZEN)
def test_commit_authors_drop_an_address_no_account_carries(http_mocker: HttpMocker) -> None:
    config = GitlabConfigBuilder().build()
    http_mocker.get(HttpRequest(_PROJECTS_URL, query_params=ANY_QUERY_PARAMS), _projects_page())
    http_mocker.get(
        HttpRequest(f"{PROXY_URL}/v1/authors", query_params=ANY_QUERY_PARAMS),
        _authors_page(_author("ci@build.local", "b" * 40)),
    )
    http_mocker.get(
        HttpRequest(f"{API_URL}/users", query_params=ANY_QUERY_PARAMS), HttpResponse(body="[]", status_code=200)
    )

    output = read_stream(_CONNECTOR, "commit_authors", config)

    assert not output.errors
    assert output.records == []
