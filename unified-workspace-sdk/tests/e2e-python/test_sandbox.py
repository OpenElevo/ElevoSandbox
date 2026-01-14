#!/usr/bin/env python3
"""E2E tests for Workspace SDK using Python standard library only"""

import os
import sys
import time
import json
import urllib.request
import urllib.error
from typing import Optional, List, Dict, Any
from dataclasses import dataclass
from contextlib import contextmanager

BASE_URL = os.environ.get("WORKSPACE_TEST_URL", "http://127.0.0.1:8080")
BASE_IMAGE = os.environ.get("WORKSPACE_BASE_IMAGE", "docker.easyops.local/ci/rust-builder:1.85.0-centos7")
API_URL = f"{BASE_URL}/api/v1"

@dataclass
class TestResult:
    name: str
    passed: bool
    error: Optional[str] = None
    duration: float = 0.0

results: List[TestResult] = []

@contextmanager
def test_case(name: str):
    """Context manager for running a test case"""
    start = time.time()
    try:
        yield
        duration = time.time() - start
        results.append(TestResult(name=name, passed=True, duration=duration))
        print(f"  ✓ {name} ({duration*1000:.0f}ms)")
    except AssertionError as e:
        duration = time.time() - start
        results.append(TestResult(name=name, passed=False, error=str(e), duration=duration))
        print(f"  ✗ {name} ({duration*1000:.0f}ms)")
        print(f"    Error: {e}")
    except Exception as e:
        duration = time.time() - start
        results.append(TestResult(name=name, passed=False, error=str(e), duration=duration))
        print(f"  ✗ {name} ({duration*1000:.0f}ms)")
        print(f"    Error: {e}")


class WorkspaceClient:
    """Simple HTTP client for Workspace API using standard library"""

    def __init__(self, base_url: str, timeout: float = 60.0):
        self.base_url = base_url
        self.api_url = f"{base_url}/api/v1"
        self.timeout = timeout

    def _request(self, method: str, path: str, data: Optional[Dict] = None) -> Dict[str, Any]:
        url = f"{self.api_url}{path}"
        headers = {"Content-Type": "application/json"}

        if data:
            body = json.dumps(data).encode("utf-8")
        else:
            body = None

        req = urllib.request.Request(url, data=body, headers=headers, method=method)

        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            # Re-raise with status code info
            raise HttpError(e.code, e.read().decode("utf-8")) from e

    def health(self) -> Dict[str, Any]:
        return self._request("GET", "/health")

    def create_sandbox(
        self,
        template: Optional[str] = None,
        name: Optional[str] = None,
        env: Optional[Dict[str, str]] = None,
        metadata: Optional[Dict[str, str]] = None,
        timeout: Optional[int] = None,
    ) -> Dict[str, Any]:
        data = {}
        if template:
            data["template"] = template
        if name:
            data["name"] = name
        if env:
            data["env"] = env
        if metadata:
            data["metadata"] = metadata
        if timeout:
            data["timeout"] = timeout

        return self._request("POST", "/sandboxes", data)

    def get_sandbox(self, sandbox_id: str) -> Dict[str, Any]:
        return self._request("GET", f"/sandboxes/{sandbox_id}")

    def list_sandboxes(self) -> Dict[str, Any]:
        return self._request("GET", "/sandboxes")

    def delete_sandbox(self, sandbox_id: str, force: bool = False) -> Dict[str, Any]:
        path = f"/sandboxes/{sandbox_id}"
        if force:
            path += "?force=true"
        return self._request("DELETE", path)


class HttpError(Exception):
    """HTTP error with status code"""
    def __init__(self, status_code: int, message: str):
        self.status_code = status_code
        self.message = message
        super().__init__(f"HTTP {status_code}: {message}")


def main():
    print("Workspace SDK E2E Tests (Python)")
    print("================================")
    print(f"Server URL: {BASE_URL}")
    print(f"Base Image: {BASE_IMAGE}\n")

    client = WorkspaceClient(BASE_URL)
    test_sandbox_id: Optional[str] = None

    try:
        # Health tests
        print("Health Tests:")

        with test_case("health check returns healthy status"):
            health = client.health()
            assert health.get("status") == "healthy", f"Expected 'healthy', got '{health.get('status')}'"
            assert "version" in health, "Version missing from health response"

        # Sandbox tests
        print("\nSandbox Tests:")

        with test_case("create sandbox with name"):
            sandbox = client.create_sandbox(
                template=BASE_IMAGE,
                name="e2e-python-test-sandbox",
            )
            assert sandbox.get("id"), "Sandbox ID is missing"
            assert sandbox.get("name") == "e2e-python-test-sandbox", f"Name mismatch: {sandbox.get('name')}"
            test_sandbox_id = sandbox["id"]

        with test_case("get sandbox by ID"):
            assert test_sandbox_id, "No sandbox ID available"
            sandbox = client.get_sandbox(test_sandbox_id)
            assert sandbox.get("id") == test_sandbox_id, "Sandbox ID mismatch"

        with test_case("list sandboxes includes created sandbox"):
            result = client.list_sandboxes()
            sandboxes = result.get("sandboxes", [])
            found = any(s.get("id") == test_sandbox_id for s in sandboxes)
            assert found, "Created sandbox not found in list"

        with test_case("create sandbox with environment variables"):
            sandbox = client.create_sandbox(
                template=BASE_IMAGE,
                name="e2e-python-env-test",
                env={"MY_VAR": "test_value", "ANOTHER_VAR": "another_value"},
            )
            env = sandbox.get("env", {})
            assert env.get("MY_VAR") == "test_value", f"ENV mismatch: {env}"
            # Cleanup
            client.delete_sandbox(sandbox["id"], force=True)

        with test_case("create sandbox with metadata"):
            sandbox = client.create_sandbox(
                template=BASE_IMAGE,
                name="e2e-python-metadata-test",
                metadata={"project": "test-project", "owner": "test-user"},
            )
            metadata = sandbox.get("metadata", {})
            assert metadata.get("project") == "test-project", f"Metadata mismatch: {metadata}"
            # Cleanup
            client.delete_sandbox(sandbox["id"], force=True)

        with test_case("delete sandbox"):
            assert test_sandbox_id, "No sandbox ID available"
            client.delete_sandbox(test_sandbox_id, force=True)

            # Verify deletion
            try:
                client.get_sandbox(test_sandbox_id)
                assert False, "Sandbox should have been deleted"
            except HttpError as e:
                assert e.status_code == 404, f"Expected 404, got {e.status_code}"
            test_sandbox_id = None

        # Error handling tests
        print("\nError Handling Tests:")

        with test_case("get non-existent sandbox returns 404"):
            try:
                client.get_sandbox("non-existent-sandbox-id")
                assert False, "Expected 404 error"
            except HttpError as e:
                assert e.status_code == 404, f"Expected 404, got {e.status_code}"

    finally:
        # Cleanup any remaining test sandbox
        if test_sandbox_id:
            try:
                client.delete_sandbox(test_sandbox_id, force=True)
            except Exception:
                pass

    # Print summary
    print("\n================================")
    passed = sum(1 for r in results if r.passed)
    failed = sum(1 for r in results if not r.passed)
    total_duration = sum(r.duration for r in results)

    print(f"Results: {passed} passed, {failed} failed")
    print(f"Total time: {total_duration*1000:.0f}ms")

    if failed > 0:
        print("\nFailed tests:")
        for r in results:
            if not r.passed:
                print(f"  - {r.name}: {r.error}")
        sys.exit(1)


if __name__ == "__main__":
    main()
