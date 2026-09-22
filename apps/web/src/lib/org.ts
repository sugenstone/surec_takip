// Mirrors the backend name rules: 1–200 characters after trimming.
export function validOrganizationName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed.length > 0 && [...trimmed].length <= 200;
}

// Workspace names follow the same backend rules: 1–200 characters after
// trimming.
export function validWorkspaceName(name: string): boolean {
  return validOrganizationName(name);
}
