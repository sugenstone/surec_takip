<script lang="ts">
  // Project rename/description edit surface rendered inside the edit drawer.
  import { invalidateAll } from '$app/navigation';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import { updateProject, type ProjectPublic } from '$lib/api/client';
  import { projectErrorMessageKey } from '$lib/api/errors';
  let {
    orgId,
    wsId,
    project,
    locale,
    onSaved,
    onCancel,
  }: {
    orgId: string;
    wsId: string;
    project: ProjectPublic;
    locale: Locale;
    onSaved: () => void;
    onCancel: () => void;
  } = $props();

  let name = $state(project.name);
  let description = $state(project.description ?? '');
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
        await updateProject(orgId, wsId, project.id, {
          name: trimmed,
          description: description.trim(),
        });
        await invalidateAll();
        onSaved();
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
  aria-describedby={error ? 'project-edit-error' : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label class="field-label" for="project-edit-name">
    {translate(locale, 'projects.create.label')}
  </label>
  <input
    id="project-edit-name"
    name="name"
    type="text"
    bind:value={name}
    required
    maxlength="200"
    autocomplete="off"
  />
  <label class="field-label" for="project-edit-description">
    {translate(locale, 'projects.detail.description')}
  </label>
  <textarea id="project-edit-description" name="description" bind:value={description} rows="3"
  ></textarea>
  {#if error}<p id="project-edit-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {pending
        ? translate(locale, 'projects.detail.saving')
        : translate(locale, 'projects.detail.save')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'projects.detail.cancel')}
    </button>
  </div>
</form>
