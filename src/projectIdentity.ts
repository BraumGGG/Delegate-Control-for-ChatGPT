import type { ProjectConfig } from "./types";

export const PROJECT_KEY_PATTERN = /^[a-z0-9](?:[a-z0-9_-]{0,63})$/;

function normalizeProjectKeyName(name: string) {
  return name.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 64) || "project";
}

export function suggestProjectKey(name: string, usedIds: Iterable<string>) {
  const used = new Set(usedIds);
  const base = normalizeProjectKeyName(name);
  let candidate = base;
  let suffix = 2;
  while (used.has(candidate)) {
    const suffixText = `-${suffix}`;
    candidate = `${base.slice(0, 64 - suffixText.length)}${suffixText}`;
    suffix += 1;
  }
  return candidate;
}

export function isLegacyGenericProjectKey(id: string) {
  return /^new-project(?:-\d+)?$/.test(id);
}

export function getLegacyProjectKeyProposal(project: ProjectConfig, projects: ProjectConfig[]) {
  if (!isLegacyGenericProjectKey(project.id)) return null;
  const proposedKey = normalizeProjectKeyName(project.name);
  return {
    proposedKey,
    conflict: projects.some((candidate) => candidate.id !== project.id && candidate.id === proposedKey),
  };
}

export function validateProjectKeyCandidate(currentId: string, candidate: string, projects: ProjectConfig[]) {
  if (!PROJECT_KEY_PATTERN.test(candidate)) {
    return "请使用小写字母、数字、短横线或下划线，长度不超过 64。";
  }
  if (candidate === currentId) return "新的 Project Key 不能与当前 key 相同。";
  if (projects.some((project) => project.id !== currentId && project.id === candidate)) {
    return `Project Key 已存在：${candidate}`;
  }
  return null;
}
