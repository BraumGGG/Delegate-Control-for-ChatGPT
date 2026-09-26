import assert from "node:assert/strict";
import test from "node:test";
import {
  getLegacyProjectKeyProposal,
  isLegacyGenericProjectKey,
  suggestProjectKey,
  validateProjectKeyCandidate,
} from "./projectIdentity.ts";
import type { ProjectConfig } from "./types.ts";

function project(id: string, name: string): ProjectConfig {
  return { id, name, output_directory: `C:\\Projects\\${id}`, mcp_host: "127.0.0.1", mcp_port: 8000, enabled: true };
}

test("detects only historical generic project keys", () => {
  assert.equal(isLegacyGenericProjectKey("new-project"), true);
  assert.equal(isLegacyGenericProjectKey("new-project-2"), true);
  assert.equal(isLegacyGenericProjectKey("new-project-27"), true);
  assert.equal(isLegacyGenericProjectKey("new-project-custom"), false);
  assert.equal(isLegacyGenericProjectKey("screencast"), false);
  assert.equal(isLegacyGenericProjectKey("project"), false);
});

test("proposes canonical keys for Realize and DCFC without flagging ScreenCast", () => {
  const projects = [project("screencast", "ScreenCast"), project("new-project", "Realize"), project("new-project-2", "DCFC")];

  assert.deepEqual(getLegacyProjectKeyProposal(projects[1], projects), { proposedKey: "realize", conflict: false });
  assert.deepEqual(getLegacyProjectKeyProposal(projects[2], projects), { proposedKey: "dcfc", conflict: false });
  assert.equal(getLegacyProjectKeyProposal(projects[0], projects), null);
});

test("reports a canonical proposal conflict instead of silently changing the target", () => {
  const projects = [project("realize", "Existing"), project("new-project", "Realize")];

  assert.deepEqual(getLegacyProjectKeyProposal(projects[1], projects), { proposedKey: "realize", conflict: true });
  assert.match(validateProjectKeyCandidate("new-project", "realize", projects) ?? "", /已存在/);
});

test("validates migration candidates and ignores the current project identity", () => {
  const projects = [project("new-project", "Realize"), project("dcfc", "DCFC")];

  assert.equal(validateProjectKeyCandidate("new-project", "realize", projects), null);
  assert.match(validateProjectKeyCandidate("new-project", "Bad Key", projects) ?? "", /小写字母/);
  assert.match(validateProjectKeyCandidate("new-project", "new-project", projects) ?? "", /当前 key 相同/);
});

test("uses deterministic unique suggestions and keeps the non-Latin fallback", () => {
  assert.equal(suggestProjectKey("Realize", ["realize"]), "realize-2");
  assert.equal(suggestProjectKey("项目", []), "project");
  assert.equal(suggestProjectKey("项目", ["project", "project-2"]), "project-3");
});
