<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import { navigating } from '$app/state';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
  import '$lib/work-items/work-items.css';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  let params = $derived({
    orgId: data.organization.id,
    wsId: data.workspace.id,
    projectId: data.project.id,
    sectionId: data.section.id,
  });
</script>

<svelte:head
  ><title>{data.section.name} — {translate(data.locale, 'workItems.title')}</title></svelte:head
>
<div class="work-items" aria-busy={!!navigating.to}>
  <a href={resolve('/app/[orgId]/[wsId]/projects/[projectId]', params)}
    >{translate(data.locale, 'workItems.project')}: {data.project.name}</a
  >
  <h1>{data.section.name} — {translate(data.locale, 'workItems.title')}</h1>
  {#if navigating.to}<p role="status">{translate(data.locale, 'workItems.loading')}</p>{/if}
  {#if data.failed}
    <p class="error" role="alert">{translate(data.locale, 'workItems.error.load')}</p>
    <button type="button" onclick={() => invalidateAll()}
      >{translate(data.locale, 'workItems.retry')}</button
    >
  {:else if data.items.length === 0}
    <p>{translate(data.locale, 'workItems.empty')}</p>
  {:else}
    <ul>
      {#each data.items as item (item.id)}
        <li>
          <a
            href={resolve(
              '/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]/work-items/[workItemId]',
              { ...params, workItemId: item.id },
            )}>{item.name}</a
          >
          <span class="item-status" class:completed={item.status === 'completed'}
            >{translate(
              data.locale,
              item.status === 'completed'
                ? 'workItems.status.completed'
                : 'workItems.status.active',
            )}</span
          >
        </li>
      {/each}
    </ul>
  {/if}
  {#if data.permissions.includes('work_items:create')}
    <section aria-label={translate(data.locale, 'workItems.create')}>
      <h2>{translate(data.locale, 'workItems.create')}</h2>
      {#key data.section.id}<WorkItemForm
          scope={data.workItemScope}
          locale={data.locale}
          onSaved={async () => {
            await invalidateAll();
          }}
        />{/key}
    </section>
  {/if}
</div>
