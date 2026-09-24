<script lang="ts">
  // Workspace creation form rendered inside the create drawer; the parent
  // navigates into the new workspace on success.
  import { invalidateAll } from '$app/navigation';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import { createWorkspace, type WorkspacePublic } from '$lib/api/client';
  import { workspaceErrorMessageKey } from '$lib/api/errors';
  let {
    orgId,
    locale,
    onCreated,
    onCancel,
  }: {
    orgId: string;
    locale: Locale;
    onCreated: (workspace: WorkspacePublic) => void;
    onCancel: () => void;
  } = $props();

  let name = $state('');
  let pending = $state(false);
  let error: string | null = $state(null);

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || pending) return;
    error = null;
    pending = true;
    (async () => {
      try {
        const workspace = await createWorkspace(orgId, trimmed);
        await invalidateAll();
        onCreated(workspace);
      } catch (cause) {
        const key: TranslationKey = workspaceErrorMessageKey(cause);
        error = translate(locale, key);
      } finally {
        pending = false;
      }
    })();
  }
</script>

<form
  aria-describedby={error ? 'workspace-create-error' : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label class="field-label" for="ws-create-name">
    {translate(locale, 'workspace.create.label')}
  </label>
  <input
    id="ws-create-name"
    name="name"
    type="text"
    bind:value={name}
    required
    maxlength="200"
    autocomplete="off"
  />
  {#if error}<p id="workspace-create-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {pending ? translate(locale, 'workspace.create.pending') : translate(locale, 'ui.create')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'ui.cancel')}
    </button>
  </div>
</form>
