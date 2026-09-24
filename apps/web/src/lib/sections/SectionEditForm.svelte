<script lang="ts">
  // Edit surface for an existing section (rename + reparent), rendered inside
  // the edit drawer. Reparent candidates exclude the section itself and its
  // descendants; the backend re-validates the move (cycle/tenant rules).
  import { invalidateAll } from '$app/navigation';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import { updateSection, type SectionPublic } from '$lib/api/client';
  import { sectionErrorMessageKey } from '$lib/api/errors';
  import { movableParents } from '$lib/sections/tree';
  let {
    orgId,
    wsId,
    projectId,
    section,
    sections,
    locale,
    onSaved,
    onCancel,
  }: {
    orgId: string;
    wsId: string;
    projectId: string;
    section: SectionPublic;
    sections: SectionPublic[];
    locale: Locale;
    onSaved: () => void;
    onCancel: () => void;
  } = $props();

  let name = $state(section.name);
  let parent = $state(section.parent_section_id ?? '');
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
        await updateSection(orgId, wsId, projectId, section.id, {
          name: trimmed,
          parentSectionId: parent === '' ? null : parent,
        });
        await invalidateAll();
        onSaved();
      } catch (cause) {
        const key: TranslationKey = sectionErrorMessageKey(cause);
        error = translate(locale, key);
      } finally {
        pending = false;
      }
    })();
  }
</script>

<form
  aria-describedby={error ? 'section-edit-error' : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label class="field-label" for="section-edit-name">
    {translate(locale, 'sections.create.label')}
  </label>
  <input
    id="section-edit-name"
    type="text"
    bind:value={name}
    required
    maxlength="200"
    autocomplete="off"
  />
  <label class="field-label" for="section-edit-parent">
    {translate(locale, 'sections.move.target')}
  </label>
  <select id="section-edit-parent" bind:value={parent}>
    <option value="">{translate(locale, 'sections.move.root')}</option>
    {#each movableParents(sections, section.id) as candidate (candidate.id)}
      <option value={candidate.id}>{candidate.name}</option>
    {/each}
  </select>
  {#if error}<p id="section-edit-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {pending ? translate(locale, 'sections.saving') : translate(locale, 'sections.save')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'sections.cancel')}
    </button>
  </div>
</form>
