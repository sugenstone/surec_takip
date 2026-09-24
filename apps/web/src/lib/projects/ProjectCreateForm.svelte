<script lang="ts">
  // Project creation form rendered inside the create drawer. On success the
  // parent decides navigation (the projects list enters the new project).
  import { invalidateAll } from '$app/navigation';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import { createProject, type ProjectPublic } from '$lib/api/client';
  import { projectErrorMessageKey } from '$lib/api/errors';
  let {
    orgId,
    wsId,
    locale,
    onCreated,
    onCancel,
  }: {
    orgId: string;
    wsId: string;
    locale: Locale;
    onCreated: (project: ProjectPublic) => void;
    onCancel: () => void;
  } = $props();

  let name = $state('');
  let description = $state('');
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
        const project = await createProject(orgId, wsId, {
          name: trimmed,
          ...(description.trim() ? { description: description.trim() } : {}),
        });
        await invalidateAll();
        onCreated(project);
      } catch (cause) {
        const key: TranslationKey = projectErrorMessageKey(cause);
        error = translate(locale, key);
      } finally {
        pending = false;
      }
    })();
  }
</script>

<form
  aria-describedby={error ? 'project-create-error' : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label class="field-label" for="project-create-name">
    {translate(locale, 'projects.create.label')}
  </label>
  <input
    id="project-create-name"
    name="name"
    type="text"
    bind:value={name}
    required
    maxlength="200"
    autocomplete="off"
  />
  <label class="field-label" for="project-create-description">
    {translate(locale, 'projects.create.description')}
  </label>
  <textarea id="project-create-description" name="description" bind:value={description} rows="2"
  ></textarea>
  {#if error}<p id="project-create-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {pending ? translate(locale, 'projects.create.pending') : translate(locale, 'ui.create')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'ui.cancel')}
    </button>
  </div>
</form>
