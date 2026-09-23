<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
  import '$lib/work-items/work-items.css';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  let listParams = $derived({
    orgId: data.organization.id,
    wsId: data.workspace.id,
    projectId: data.project.id,
    sectionId: data.section.id,
  });
</script>

<svelte:head
  ><title>{data.item.name} — {translate(data.locale, 'workItems.title')}</title></svelte:head
>
<div class="work-items">
  <a
    href={resolve(
      '/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]/work-items',
      listParams,
    )}>{translate(data.locale, 'workItems.back')}: {data.section.name}</a
  >
  <h1>{data.item.name}</h1>
  <p>{data.project.name} / {data.section.name}</p>
  <p class="item-status" class:completed={data.item.status === 'completed'}>
    {translate(
      data.locale,
      data.item.status === 'completed' ? 'workItems.status.completed' : 'workItems.status.active',
    )}
  </p>
  {#if data.permissions.includes('work_items:update')}
    <h2>{translate(data.locale, 'workItems.edit')}</h2>
    {#key data.item.id}<WorkItemForm
        scope={data.workItemScope}
        locale={data.locale}
        item={data.item}
        canArchive={data.permissions.includes('work_items:archive')}
        onSaved={async (item) => {
          if (item.status === 'archived')
            await goto(
              resolve(
                '/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]/work-items',
                listParams,
              ),
            );
          else await invalidateAll();
        }}
      />{/key}
  {/if}
</div>
