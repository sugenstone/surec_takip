<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import SectionBreadcrumbs from '$lib/ui/SectionBreadcrumbs.svelte';
  import { translate } from '$lib/i18n';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
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
  <PageHeader title={data.item.name}>
    {#snippet context()}<SectionBreadcrumbs
        locale={data.locale}
        organization={data.organization}
        workspace={data.workspace}
        project={data.project}
        trail={data.sectionTrail}
        leaf={data.item.name}
      />{/snippet}
    {#snippet metadata()}<StatusBadge
        status={data.item.status}
        label={translate(
          data.locale,
          data.item.status === 'completed'
            ? 'workItems.status.completed'
            : 'workItems.status.active',
        )}
      />{/snippet}
  </PageHeader>
  {#if data.permissions.includes('work_items:update')}
    <section class="form-panel">
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
                  '/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]',
                  listParams,
                ),
              );
            else await invalidateAll();
          }}
        />{/key}
    </section>
  {:else}<section class="panel">
      <p class="muted">{translate(data.locale, 'ui.readOnly')}</p>
      <p>{translate(data.locale, 'workItems.slug')}: {data.item.slug}</p>
    </section>{/if}
</div>
