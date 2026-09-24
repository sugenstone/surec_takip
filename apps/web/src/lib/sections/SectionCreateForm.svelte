<script lang="ts">
  // Creates a section under the CURRENT hierarchy context: `parentId` null on
  // the project page creates a root; a section id on a section page creates a
  // direct child. The backend re-validates the parent — the URL context is
  // never an authorization boundary. Rendered inside the create drawer
  // (FormDrawer), which provides the title and dismissal chrome. The drawer
  // only mounts children client-side, so no SSR/hydration gating is needed.
  import { invalidateAll } from '$app/navigation';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import { createSection } from '$lib/api/client';
  import { sectionErrorMessageKey } from '$lib/api/errors';
  let {
    orgId,
    wsId,
    projectId,
    parentId,
    locale,
    inputId,
    onCreated,
    onCancel,
  }: {
    orgId: string;
    wsId: string;
    projectId: string;
    parentId: string | null;
    locale: Locale;
    inputId: string;
    onCreated: () => void;
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
        await createSection(orgId, wsId, projectId, {
          name: trimmed,
          parentSectionId: parentId,
        });
        await invalidateAll();
        onCreated();
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
  aria-describedby={error ? `${inputId}-error` : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label class="field-label" for={inputId}>{translate(locale, 'sections.create.label')}</label>
  <input id={inputId} bind:value={name} required maxlength="200" autocomplete="off" />
  {#if error}<p id="{inputId}-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {pending ? translate(locale, 'sections.create.pending') : translate(locale, 'ui.create')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'ui.cancel')}
    </button>
  </div>
</form>
