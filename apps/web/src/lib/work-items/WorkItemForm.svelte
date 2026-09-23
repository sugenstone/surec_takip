<script lang="ts">
  import { untrack } from 'svelte';
  import { translate, type Locale } from '$lib/i18n';
  import {
    createWorkItem,
    updateWorkItem,
    type WorkItemScope,
    type WorkItemPublic,
  } from '$lib/api/client';
  import { workItemErrorMessageKey } from '$lib/api/errors';
  let {
    scope,
    locale,
    item,
    canArchive = false,
    onSaved,
  }: {
    scope: WorkItemScope;
    locale: Locale;
    item?: WorkItemPublic;
    canArchive?: boolean;
    onSaved: (item: WorkItemPublic) => Promise<void>;
  } = $props();
  // The keyed parent creates a fresh draft on identity changes. Failed saves
  // retain it; background load updates do not overwrite an in-progress edit.
  let name = $state(untrack(() => item?.name ?? ''));
  let slug = $state(untrack(() => item?.slug ?? ''));
  let position = $state<number | undefined>(untrack(() => item?.position ?? 0));
  let status = $state(untrack(() => item?.status ?? 'active'));
  let pending = $state(false);
  let message = $state('');
  let saved = $state(false);
  let confirming = $state(false);
  async function save(archive = false) {
    if (pending) return;
    message = '';
    saved = false;
    pending = true;
    try {
      const result = item
        ? await updateWorkItem(
            scope,
            item.id,
            archive
              ? { status: 'archived' }
              : {
                  name,
                  slug,
                  position,
                  status,
                },
          )
        : await createWorkItem(scope, { name, slug: slug.trim() || undefined });
      await onSaved(result);
      if (!item) {
        name = '';
        slug = '';
      }
      saved = true;
    } catch (error) {
      message = translate(locale, workItemErrorMessageKey(error));
    } finally {
      pending = false;
    }
  }
</script>

<form
  onsubmit={(event) => {
    event.preventDefault();
    save();
  }}
  aria-busy={pending}
>
  <label for="work-item-name">{translate(locale, 'workItems.name')}</label>
  <input id="work-item-name" bind:value={name} required maxlength="200" autocomplete="off" />
  <label for="work-item-slug">{translate(locale, 'workItems.slug')}</label>
  <input id="work-item-slug" bind:value={slug} maxlength="64" autocomplete="off" />
  {#if item}
    <label for="work-item-position">{translate(locale, 'workItems.position')}</label>
    <input
      id="work-item-position"
      type="number"
      bind:value={position}
      min="0"
      max="2147483647"
      step="1"
      required
    />
    <label for="work-item-status">{translate(locale, 'workItems.status')}</label>
    <select id="work-item-status" bind:value={status}>
      <option value="active">{translate(locale, 'workItems.status.active')}</option>
      <option value="completed">{translate(locale, 'workItems.status.completed')}</option>
    </select>
  {/if}
  {#if message}<p class="error" role="alert">{message}</p>{/if}
  {#if saved}<p role="status">{translate(locale, 'workItems.saved')}</p>{/if}
  <button disabled={pending || !name.trim()} type="submit"
    >{translate(
      locale,
      pending ? 'workItems.saving' : item ? 'workItems.save' : 'workItems.create',
    )}</button
  >
</form>
{#if item && canArchive}
  {#if confirming}
    <div class="archive-confirm">
      <p>{translate(locale, 'workItems.archive.confirm')}</p>
      <button type="button" disabled={pending} onclick={() => save(true)}
        >{translate(locale, 'workItems.archive.submit')}</button
      >
      <button
        type="button"
        disabled={pending}
        onclick={() => {
          confirming = false;
        }}>{translate(locale, 'workItems.cancel')}</button
      >
    </div>
  {:else}
    <button
      class="secondary"
      type="button"
      disabled={pending}
      onclick={() => {
        confirming = true;
      }}>{translate(locale, 'workItems.archive')}</button
    >
  {/if}
{/if}
