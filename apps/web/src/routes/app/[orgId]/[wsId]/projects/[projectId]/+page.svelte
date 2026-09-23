<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { invalidateAll } from '$app/navigation';
  import { updateProject, createSection, updateSection } from '$lib/api/client';
  import { projectErrorMessageKey, sectionErrorMessageKey } from '$lib/api/errors';
  import { buildSectionTree, flattenTree, movableParents } from '$lib/sections/tree';
  import type { TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let canUpdateProject = $derived(data.permissions.includes('projects:update'));
  let canArchiveProject = $derived(data.permissions.includes('projects:archive'));
  let canCreateSections = $derived(data.permissions.includes('sections:create'));
  let canUpdateSections = $derived(data.permissions.includes('sections:update'));
  let canArchiveSections = $derived(data.permissions.includes('sections:archive'));

  // ---- project lifecycle -------------------------------------------------
  let editingProject = $state(false);
  let projectName = $state('');
  let projectDescription = $state('');
  let projectPending = $state(false);
  let projectError: string | null = $state(null);

  // ---- sections ----------------------------------------------------------
  let rows = $derived(flattenTree(buildSectionTree(data.sections)));
  let sectionError: string | null = $state(null);
  let sectionPending = $state(false);
  let rootName = $state('');
  let renamingId = $state<string | null>(null);
  let renameValue = $state('');
  let childCreatingId = $state<string | null>(null);
  let childName = $state('');
  let movingId = $state<string | null>(null);
  let moveTarget = $state('');

  const statusKey = (status: string): TranslationKey =>
    `projects.status.${status}` as TranslationKey;
  const sectionStatusKey = (status: string): TranslationKey =>
    `sections.status.${status}` as TranslationKey;

  async function runSectionAction(action: () => Promise<unknown>) {
    if (sectionPending) return;
    sectionError = null;
    sectionPending = true;
    try {
      await action();
      await invalidateAll();
    } catch (error) {
      const key: TranslationKey = sectionErrorMessageKey(error);
      sectionError = translate(locale, key);
    } finally {
      sectionPending = false;
    }
  }

  function base() {
    return {
      org: data.organization.id,
      ws: data.workspace.id,
      project: data.project.id,
    };
  }

  function addRoot(event: SubmitEvent) {
    event.preventDefault();
    const name = rootName.trim();
    if (!name) return;
    runSectionAction(async () => {
      await createSection(base().org, base().ws, base().project, { name });
      rootName = '';
    });
  }

  function addChild(parentId: string, event: SubmitEvent) {
    event.preventDefault();
    const name = childName.trim();
    if (!name) return;
    runSectionAction(async () => {
      await createSection(base().org, base().ws, base().project, {
        name,
        parentSectionId: parentId,
      });
      childCreatingId = null;
      childName = '';
    });
  }

  function rename(sectionId: string, event: SubmitEvent) {
    event.preventDefault();
    const name = renameValue.trim();
    if (!name) return;
    runSectionAction(async () => {
      await updateSection(base().org, base().ws, base().project, sectionId, { name });
      renamingId = null;
    });
  }

  function reparent(sectionId: string, parentSectionId: string | null) {
    runSectionAction(() =>
      updateSection(base().org, base().ws, base().project, sectionId, { parentSectionId }),
    );
    movingId = null;
    moveTarget = '';
  }

  function setSectionStatus(sectionId: string, status: 'active' | 'archived') {
    runSectionAction(() =>
      updateSection(base().org, base().ws, base().project, sectionId, { status }),
    );
  }

  // Sibling reorder: positions are the single ordering primitive; each PATCH
  // is independently transactional (the backend appends on reparent without
  // an explicit position).
  function reorder(sectionId: string, position: number) {
    runSectionAction(() =>
      updateSection(base().org, base().ws, base().project, sectionId, { position }),
    );
  }

  function saveProject(event: SubmitEvent) {
    event.preventDefault();
    if (projectPending) return;
    projectError = null;
    projectPending = true;
    (async () => {
      try {
        await updateProject(base().org, base().ws, base().project, {
          name: projectName.trim(),
          description: projectDescription.trim(),
        });
        editingProject = false;
        await invalidateAll();
      } catch (error) {
        const key: TranslationKey = projectErrorMessageKey(error);
        projectError = translate(locale, key);
      } finally {
        projectPending = false;
      }
    })();
  }

  function transition(status: 'completed' | 'archived' | 'active') {
    if (projectPending) return;
    projectError = null;
    projectPending = true;
    (async () => {
      try {
        await updateProject(base().org, base().ws, base().project, { status });
        await invalidateAll();
      } catch (error) {
        const key: TranslationKey = projectErrorMessageKey(error);
        projectError = translate(locale, key);
      } finally {
        projectPending = false;
      }
    })();
  }
</script>

<svelte:head>
  <title>{data.project.name} — {data.workspace.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

<div class="header">
  <h1>{data.project.name}</h1>
  <span
    class="status-badge status-{data.project.status}"
    aria-label="{translate(locale, 'projects.column.status')}: {translate(
      locale,
      statusKey(data.project.status),
    )}"
  >
    {translate(locale, statusKey(data.project.status))}
  </span>
</div>
<p class="slug">{translate(locale, 'projects.detail.slug')}: {data.project.slug}</p>

{#if data.project.description}
  <section class="description">
    <h2>{translate(locale, 'projects.detail.description')}</h2>
    <p>{data.project.description}</p>
  </section>
{/if}

{#if canUpdateProject}
  {#if editingProject}
    <section class="edit-section" aria-label={translate(locale, 'projects.detail.edit')}>
      <h2>{translate(locale, 'projects.detail.edit')}</h2>
      <form onsubmit={saveProject} method="post" novalidate>
        <label class="field-label" for="project-edit-name">
          {translate(locale, 'projects.create.label')}
        </label>
        <input
          id="project-edit-name"
          name="name"
          type="text"
          bind:value={projectName}
          required
          maxlength="200"
          autocomplete="off"
        />
        <label class="field-label" for="project-edit-description">
          {translate(locale, 'projects.detail.description')}
        </label>
        <textarea
          id="project-edit-description"
          name="description"
          bind:value={projectDescription}
          rows="3"
        ></textarea>
        <div class="button-row">
          <button type="submit" disabled={projectPending || projectName.trim().length === 0}>
            {projectPending
              ? translate(locale, 'projects.detail.saving')
              : translate(locale, 'projects.detail.save')}
          </button>
          <button
            type="button"
            class="secondary"
            onclick={() => {
              editingProject = false;
              projectError = null;
            }}
          >
            {translate(locale, 'projects.detail.cancel')}
          </button>
        </div>
      </form>
    </section>
  {:else}
    <button
      type="button"
      onclick={() => {
        projectName = data.project.name;
        projectDescription = data.project.description ?? '';
        editingProject = true;
      }}
    >
      {translate(locale, 'projects.detail.edit')}
    </button>
  {/if}
{/if}

<div class="button-row status-actions">
  {#if data.project.status === 'active' && canUpdateProject}
    <button type="button" disabled={projectPending} onclick={() => transition('completed')}>
      {translate(locale, 'projects.detail.complete')}
    </button>
  {/if}
  {#if data.project.status === 'completed' && canUpdateProject}
    <button type="button" disabled={projectPending} onclick={() => transition('active')}>
      {translate(locale, 'projects.detail.reopen')}
    </button>
  {/if}
  {#if data.project.status === 'archived' && canUpdateProject}
    <button type="button" disabled={projectPending} onclick={() => transition('active')}>
      {translate(locale, 'projects.detail.reactivate')}
    </button>
  {/if}
  {#if data.project.status !== 'archived' && canArchiveProject}
    <button
      type="button"
      class="secondary"
      disabled={projectPending}
      onclick={() => transition('archived')}
    >
      {translate(locale, 'projects.detail.archive')}
    </button>
  {/if}
</div>

{#if projectError}
  <p class="error" role="alert">{projectError}</p>
{/if}

<section class="sections" aria-label={translate(locale, 'sections.title')}>
  <h2>{translate(locale, 'sections.title')}</h2>

  {#if data.sectionsFailed}
    <p class="error" role="alert">{translate(locale, 'sections.error.network')}</p>
  {:else if data.sections.length === 0}
    <div class="empty-section">
      <h3>{translate(locale, 'sections.empty.title')}</h3>
      <p>{translate(locale, 'sections.empty.description')}</p>
    </div>
  {:else}
    <ul class="section-list" role="list">
      {#each rows as row (row.section.id)}
        <li
          class="section-row status-{row.section.status}"
          style="padding-inline-start: calc(var(--space-4) + {row.depth} * var(--space-5))"
        >
          {#if renamingId === row.section.id}
            <form
              class="inline-form"
              onsubmit={(event) => rename(row.section.id, event)}
              method="post"
              novalidate
            >
              <label class="sr-only" for="rename-{row.section.id}">
                {translate(locale, 'sections.create.label')}
              </label>
              <input
                id="rename-{row.section.id}"
                bind:value={renameValue}
                required
                maxlength="200"
                autocomplete="off"
              />
              <button type="submit" disabled={sectionPending}>
                {translate(locale, 'sections.save')}
              </button>
              <button type="button" class="secondary" onclick={() => (renamingId = null)}>
                {translate(locale, 'sections.cancel')}
              </button>
            </form>
          {:else}
            <span class="section-name">{row.section.name}</span>
            {#if row.section.status === 'archived'}
              <span class="section-status">
                {translate(locale, sectionStatusKey(row.section.status))}
              </span>
            {/if}
            {#if canUpdateSections}
              <span class="row-actions">
                <button
                  type="button"
                  class="mini"
                  disabled={sectionPending}
                  onclick={() => {
                    renamingId = row.section.id;
                    renameValue = row.section.name;
                  }}
                >
                  {translate(locale, 'sections.rename')}
                </button>
                {#if canCreateSections}
                  <button
                    type="button"
                    class="mini"
                    disabled={sectionPending}
                    onclick={() => {
                      childCreatingId = childCreatingId === row.section.id ? null : row.section.id;
                      childName = '';
                    }}
                  >
                    {translate(locale, 'sections.createChild')}
                  </button>
                {/if}
                {#if row.section.parent_section_id !== null}
                  <button
                    type="button"
                    class="mini"
                    disabled={sectionPending}
                    onclick={() => reparent(row.section.id, null)}
                  >
                    {translate(locale, 'sections.makeRoot')}
                  </button>
                {/if}
                <button
                  type="button"
                  class="mini"
                  disabled={sectionPending}
                  onclick={() => {
                    movingId = movingId === row.section.id ? null : row.section.id;
                    moveTarget = '';
                  }}
                >
                  {translate(locale, 'sections.move')}
                </button>
                <button
                  type="button"
                  class="mini"
                  disabled={sectionPending || row.section.position === 0}
                  onclick={() => reorder(row.section.id, row.section.position - 1)}
                >
                  {translate(locale, 'sections.up')}
                </button>
                <button
                  type="button"
                  class="mini"
                  disabled={sectionPending}
                  onclick={() => reorder(row.section.id, row.section.position + 1)}
                >
                  {translate(locale, 'sections.down')}
                </button>
                {#if row.section.status === 'active' && canArchiveSections}
                  <button
                    type="button"
                    class="mini"
                    disabled={sectionPending}
                    onclick={() => setSectionStatus(row.section.id, 'archived')}
                  >
                    {translate(locale, 'sections.archive')}
                  </button>
                {/if}
                {#if row.section.status === 'archived'}
                  <button
                    type="button"
                    class="mini"
                    disabled={sectionPending}
                    onclick={() => setSectionStatus(row.section.id, 'active')}
                  >
                    {translate(locale, 'sections.reactivate')}
                  </button>
                {/if}
              </span>
            {/if}
          {/if}

          {#if childCreatingId === row.section.id && canCreateSections}
            <form
              class="inline-form child-form"
              onsubmit={(event) => addChild(row.section.id, event)}
              method="post"
              novalidate
            >
              <label class="sr-only" for="child-{row.section.id}">
                {translate(locale, 'sections.create.label')}
              </label>
              <input
                id="child-{row.section.id}"
                bind:value={childName}
                required
                maxlength="200"
                autocomplete="off"
              />
              <button type="submit" disabled={sectionPending || childName.trim().length === 0}>
                {translate(locale, 'sections.create.submit')}
              </button>
            </form>
          {/if}

          {#if movingId === row.section.id && canUpdateSections}
            <form
              class="inline-form move-form"
              onsubmit={(event) => {
                event.preventDefault();
                reparent(row.section.id, moveTarget === '' ? null : moveTarget);
              }}
              method="post"
              novalidate
            >
              <label class="sr-only" for="move-{row.section.id}">
                {translate(locale, 'sections.move.target')}
              </label>
              <select id="move-{row.section.id}" bind:value={moveTarget}>
                <option value="">{translate(locale, 'sections.move.root')}</option>
                {#each movableParents(data.sections, row.section.id) as candidate (candidate.id)}
                  <option value={candidate.id}>{candidate.name}</option>
                {/each}
              </select>
              <button type="submit" disabled={sectionPending}>
                {translate(locale, 'sections.move')}
              </button>
            </form>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if canCreateSections}
    <form class="root-form" onsubmit={addRoot} method="post" novalidate>
      <label class="field-label" for="section-root-name">
        {translate(locale, 'sections.create.label')}
      </label>
      <div class="create-row">
        <input
          id="section-root-name"
          bind:value={rootName}
          required
          maxlength="200"
          autocomplete="off"
        />
        <button type="submit" disabled={sectionPending || rootName.trim().length === 0}>
          {sectionPending
            ? translate(locale, 'sections.create.pending')
            : translate(locale, 'sections.create.submit')}
        </button>
      </div>
    </form>
  {/if}

  {#if sectionError}
    <p class="error" role="alert">{sectionError}</p>
  {/if}
</section>

<a class="back" href={resolve(`/app/${data.organization.id}/${data.workspace.id}/projects`)}>
  ← {translate(locale, 'projects.title')}
</a>

<style>
  .header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  h1 {
    font-size: var(--text-title);
    margin-block: 0 var(--space-1);
  }
  h2 {
    font-size: var(--text-heading);
    margin-block: var(--space-6) var(--space-2);
  }
  .slug {
    color: var(--muted-foreground);
    font-size: var(--text-small);
    margin-block: 0 var(--space-4);
  }
  .status-badge {
    font-size: var(--text-small);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    white-space: nowrap;
  }
  .status-badge.status-active {
    border-color: var(--status-active);
    color: var(--status-active);
  }
  .status-badge.status-completed {
    border-color: var(--status-completed);
    color: var(--status-completed);
  }
  .status-badge.status-archived {
    opacity: 0.75;
  }
  .description {
    max-width: 40rem;
  }
  .description p {
    margin-block: 0;
    white-space: pre-line;
  }
  .edit-section {
    max-width: 28rem;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    margin-block: var(--space-4);
  }
  .field-label {
    display: block;
    font-size: var(--text-small);
    font-weight: 600;
    margin-block: var(--space-3) var(--space-2);
  }
  input,
  textarea,
  select {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    padding: var(--space-3);
    min-height: 44px;
  }
  .button-row {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-block-start: var(--space-4);
  }
  button {
    border: none;
    border-radius: var(--radius-md);
    background: var(--primary);
    color: var(--primary-foreground);
    font: inherit;
    font-weight: 600;
    padding: var(--space-3) var(--space-4);
    min-height: 44px;
    cursor: pointer;
  }
  button.secondary,
  button.mini {
    background: var(--surface-muted, var(--surface));
    color: var(--foreground);
    border: 1px solid var(--border);
  }
  button.mini {
    font-weight: 500;
    font-size: var(--text-small);
    padding: var(--space-1) var(--space-2);
    min-height: 32px;
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .status-actions {
    margin-block-start: var(--space-6);
  }
  .error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: var(--space-4) 0 0;
    max-width: 40rem;
  }
  .sections {
    margin-block: var(--space-8) var(--space-6);
  }
  .empty-section {
    max-width: 32rem;
    padding: var(--space-6);
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
  }
  .empty-section h3 {
    font-size: var(--text-body);
    font-weight: 600;
    margin-block: 0 var(--space-1);
  }
  .empty-section p {
    color: var(--muted-foreground);
    margin-block: 0;
  }
  .section-list {
    list-style: none;
    margin: 0 0 var(--space-4);
    padding: 0;
    max-width: 48rem;
  }
  .section-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding-block: var(--space-2);
    border-block-end: 1px solid var(--border);
  }
  .section-row.status-archived .section-name {
    color: var(--muted-foreground);
  }
  .section-name {
    font-weight: 500;
  }
  .section-status {
    font-size: var(--text-small);
    color: var(--muted-foreground);
  }
  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-inline-start: auto;
  }
  .inline-form {
    display: flex;
    gap: var(--space-2);
    flex-basis: 100%;
    align-items: center;
  }
  .inline-form input,
  .inline-form select {
    flex: 1;
    max-width: 24rem;
  }
  .root-form {
    max-width: 28rem;
  }
  .create-row {
    display: flex;
    gap: var(--space-2);
  }
  .create-row input {
    flex: 1;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
  .back {
    display: inline-block;
    color: var(--primary);
    text-decoration: none;
  }
</style>
