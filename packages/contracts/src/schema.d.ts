export interface paths {
    "/api/v1/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["logout"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/auth/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/health": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["health"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/invitations/accept": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["accept_invitation"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_organizations"];
        put?: never;
        post: operations["create_organization"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_organization"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/effective-permissions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * Effective organization-scope permission keys for the authenticated user.
         *     Frontend uses this for action visibility only; the backend remains the
         *     authorization authority (ADR 0010).
         */
        get: operations["effective_permissions"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/invitations": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_invitations"];
        put?: never;
        post: operations["create_invitation"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/invitations/{invitation_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["revoke_invitation"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/permissions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_permissions"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/roles": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_roles"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_workspaces"];
        put?: never;
        post: operations["create_workspace"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_workspace"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/effective-permissions": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /**
         * Effective workspace-scope permission keys for the authenticated user:
         *     organization-wide grants (workspace_id IS NULL at organization scope)
         *     plus grants bound to this workspace. Mirrors the authorize_workspace
         *     join semantics so workspace-scoped project roles surface correctly.
         *     Frontend uses this for action visibility only (ADR 0011).
         */
        get: operations["effective_workspace_permissions"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_projects"];
        put?: never;
        post: operations["create_project_handler"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_project"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: operations["update_project_handler"];
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_sections"];
        put?: never;
        post: operations["create_section_handler"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_section"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: operations["update_section_handler"];
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["list_work_items"];
        put?: never;
        post: operations["create_work_item_handler"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/api/v1/organizations/{organization_id}/workspaces/{workspace_id}/projects/{project_id}/sections/{section_id}/work-items/{work_item_id}": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_work_item"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch: operations["update_work_item_handler"];
        trace?: never;
    };
    "/api/v1/ready": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["ready"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        AcceptInvitationData: {
            /** Format: uuid */
            organization_id: string;
        };
        AcceptInvitationRequest: {
            token: string;
        };
        AcceptInvitationResponse: {
            data: components["schemas"]["AcceptInvitationData"];
        };
        AckData: Record<string, never>;
        AckResponse: {
            data: components["schemas"]["AckData"];
        };
        CreateInvitationData: {
            invitation: components["schemas"]["InvitationPublic"];
            /**
             * @description Raw invitation token. Shown ONCE at creation because email delivery
             *     is not built yet; list endpoints never return it. The inviter is
             *     responsible for delivering it to the recipient over a trusted channel.
             */
            token: string;
        };
        CreateInvitationRequest: {
            email: string;
            /**
             * Format: int64
             * @description Optional expiration in hours; defaults to 72, bounded to 720.
             */
            expires_in_hours?: number | null;
        };
        CreateInvitationResponse: {
            data: components["schemas"]["CreateInvitationData"];
        };
        CreateOrganizationData: {
            membership: components["schemas"]["MembershipPublic"];
            organization: components["schemas"]["OrganizationPublic"];
        };
        CreateOrganizationRequest: {
            name: string;
            slug?: string | null;
        };
        CreateOrganizationResponse: {
            data: components["schemas"]["CreateOrganizationData"];
        };
        CreateProjectRequest: {
            description?: string | null;
            name: string;
            slug?: string | null;
        };
        CreateSectionRequest: {
            name: string;
            /**
             * Format: uuid
             * @description Optional parent; omitted or null creates a ROOT section.
             */
            parent_section_id?: string | null;
            slug?: string | null;
        };
        CreateWorkItemRequest: {
            name: string;
            slug?: string | null;
        };
        CreateWorkspaceData: {
            membership: components["schemas"]["WorkspaceMembershipPublic"];
            workspace: components["schemas"]["WorkspacePublic"];
        };
        CreateWorkspaceRequest: {
            name: string;
            slug?: string | null;
        };
        CreateWorkspaceResponse: {
            data: components["schemas"]["CreateWorkspaceData"];
        };
        EffectivePermissionsData: {
            permissions: string[];
        };
        EffectivePermissionsResponse: {
            data: components["schemas"]["EffectivePermissionsData"];
        };
        ErrorBody: {
            code: components["schemas"]["ErrorCode"];
            details: unknown;
            message: string;
            request_id: string;
        };
        /** @enum {string} */
        ErrorCode: "RESOURCE_NOT_FOUND" | "METHOD_NOT_ALLOWED" | "SERVICE_NOT_READY" | "AUTH_REQUIRED" | "AUTH_INVALID_CREDENTIALS" | "PERMISSION_DENIED" | "INVITATION_INVALID" | "VALIDATION_ERROR" | "INTERNAL_ERROR";
        ErrorEnvelope: {
            error: components["schemas"]["ErrorBody"];
        };
        HealthData: {
            status: components["schemas"]["HealthStatus"];
        };
        HealthResponse: {
            data: components["schemas"]["HealthData"];
        };
        /** @enum {string} */
        HealthStatus: "ok" | "ready";
        InvitationListResponse: {
            data: components["schemas"]["InvitationPublic"][];
        };
        InvitationPublic: {
            accepted_at?: string | null;
            created_at: string;
            email: string;
            expires_at: string;
            /** Format: uuid */
            id: string;
            /** Format: uuid */
            invited_by_user_id: string;
            /** Format: uuid */
            organization_id: string;
            revoked_at?: string | null;
        };
        LoginData: {
            user: components["schemas"]["UserPublic"];
        };
        LoginRequest: {
            email: string;
            password: string;
        };
        LoginResponse: {
            data: components["schemas"]["LoginData"];
        };
        MeData: {
            organizations: components["schemas"]["OrganizationSummary"][];
            user: components["schemas"]["UserPublic"];
        };
        MeResponse: {
            data: components["schemas"]["MeData"];
        };
        MembershipPublic: {
            /** Format: uuid */
            id: string;
            joined_at?: string | null;
            /** Format: uuid */
            organization_id: string;
            status: string;
            /** Format: uuid */
            user_id: string;
        };
        OrganizationListResponse: {
            data: components["schemas"]["OrganizationPublic"][];
        };
        OrganizationPublic: {
            default_currency?: string | null;
            default_locale?: string | null;
            default_timezone: string;
            /** Format: uuid */
            id: string;
            name: string;
            slug: string;
            status: string;
        };
        OrganizationSummary: {
            /** Format: uuid */
            id: string;
            name: string;
            role_summary: string[];
            slug: string;
        };
        PermissionListResponse: {
            data: components["schemas"]["PermissionPublic"][];
        };
        PermissionPublic: {
            description: string;
            key: string;
        };
        ProjectListResponse: {
            data: components["schemas"]["ProjectPublic"][];
        };
        ProjectMutationResponse: {
            data: components["schemas"]["ProjectPublic"];
        };
        ProjectPublic: {
            description?: string | null;
            /** Format: uuid */
            id: string;
            name: string;
            /** Format: uuid */
            organization_id: string;
            slug: string;
            status: string;
            /** Format: uuid */
            workspace_id: string;
        };
        RoleListResponse: {
            data: components["schemas"]["RolePublic"][];
        };
        RolePublic: {
            description?: string | null;
            /** Format: uuid */
            id: string;
            is_system: boolean;
            name: string;
        };
        /**
         * @description Flat ordered list (ADR 0012): one query per project, siblings pre-sorted;
         *     clients assemble the tree in memory. Deliberately NOT a nested recursive
         *     DTO — codegen ergonomics and future move/reorder stability win.
         */
        SectionListResponse: {
            data: components["schemas"]["SectionPublic"][];
        };
        SectionMutationResponse: {
            data: components["schemas"]["SectionPublic"];
        };
        SectionPublic: {
            /** Format: uuid */
            id: string;
            name: string;
            /** Format: uuid */
            organization_id: string;
            /** Format: uuid */
            parent_section_id?: string | null;
            /** Format: int32 */
            position: number;
            /** Format: uuid */
            project_id: string;
            slug: string;
            status: string;
            /** Format: uuid */
            workspace_id: string;
        };
        /**
         * @description Ownership (tenant/workspace) and lifecycle authority never enter the body:
         *     scope comes from the authenticated session plus the route, and new
         *     projects always start `active` (ADR 0011).
         */
        UpdateProjectRequest: {
            /** @description Empty string clears the stored description. */
            description?: string | null;
            name?: string | null;
            slug?: string | null;
            status?: string | null;
        };
        UpdateSectionRequest: {
            name?: string | null;
            /**
             * Format: uuid
             * @description Absent = keep; null = become a root section; UUID = reparent.
             */
            parent_section_id?: string | null;
            /**
             * Format: int32
             * @description Sibling order; ignored when absent (a reparent appends instead).
             */
            position?: number | null;
            slug?: string | null;
            status?: string | null;
        };
        UpdateWorkItemRequest: {
            name?: string | null;
            /** Format: int32 */
            position?: number | null;
            slug?: string | null;
            status?: string | null;
        };
        UserPublic: {
            display_name: string;
            email: string;
            /** Format: uuid */
            id: string;
            locale?: string | null;
            timezone?: string | null;
        };
        WorkItemListResponse: {
            data: components["schemas"]["WorkItemPublic"][];
        };
        WorkItemMutationResponse: {
            data: components["schemas"]["WorkItemPublic"];
        };
        WorkItemPublic: {
            /** Format: uuid */
            id: string;
            name: string;
            /** Format: uuid */
            organization_id: string;
            /** Format: int32 */
            position: number;
            /** Format: uuid */
            project_id: string;
            /** Format: uuid */
            section_id: string;
            slug: string;
            status: string;
            /** Format: uuid */
            workspace_id: string;
        };
        WorkspaceListResponse: {
            data: components["schemas"]["WorkspacePublic"][];
        };
        WorkspaceMembershipPublic: {
            /** Format: uuid */
            id: string;
            /** Format: uuid */
            organization_id: string;
            status: string;
            /** Format: uuid */
            user_id: string;
            /** Format: uuid */
            workspace_id: string;
        };
        WorkspacePublic: {
            /** Format: uuid */
            id: string;
            name: string;
            /** Format: uuid */
            organization_id: string;
            slug: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LoginRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["LoginResponse"];
                };
            };
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    logout: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AckResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["MeResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    health: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HealthResponse"];
                };
            };
        };
    };
    accept_invitation: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AcceptInvitationRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AcceptInvitationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_organizations: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OrganizationListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_organization: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateOrganizationRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateOrganizationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    get_organization: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["OrganizationPublic"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    effective_permissions: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EffectivePermissionsResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_invitations: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["InvitationListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_invitation: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateInvitationRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateInvitationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    revoke_invitation: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
                /** @description Invitation id */
                invitation_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AckResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_permissions: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["PermissionListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_roles: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["RoleListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_workspaces: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkspaceListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_workspace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Organization id */
                organization_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateWorkspaceRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["CreateWorkspaceResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    get_workspace: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Workspace id */
                workspace_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkspacePublic"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    effective_workspace_permissions: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Workspace id */
                workspace_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["EffectivePermissionsResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_projects: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ProjectListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_project_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateProjectRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ProjectMutationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    get_project: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Project id */
                project_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ProjectPublic"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    update_project_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Project id */
                project_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateProjectRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ProjectMutationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_sections: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Parent project id */
                project_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SectionListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_section_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Parent project id */
                project_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateSectionRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SectionMutationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    get_section: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Parent project id */
                project_id: string;
                /** @description Section id */
                section_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SectionPublic"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    update_section_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                /** @description Parent organization id */
                organization_id: string;
                /** @description Parent workspace id */
                workspace_id: string;
                /** @description Parent project id */
                project_id: string;
                /** @description Section id */
                section_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateSectionRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["SectionMutationResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    list_work_items: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                organization_id: string;
                workspace_id: string;
                project_id: string;
                section_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkItemListResponse"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    create_work_item_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                organization_id: string;
                workspace_id: string;
                project_id: string;
                section_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["CreateWorkItemRequest"];
            };
        };
        responses: {
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkItemMutationResponse"];
                };
            };
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    get_work_item: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                organization_id: string;
                workspace_id: string;
                project_id: string;
                section_id: string;
                work_item_id: string;
            };
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkItemPublic"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    update_work_item_handler: {
        parameters: {
            query?: never;
            header?: never;
            path: {
                organization_id: string;
                workspace_id: string;
                project_id: string;
                section_id: string;
                work_item_id: string;
            };
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["UpdateWorkItemRequest"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["WorkItemMutationResponse"];
                };
            };
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            403: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
            422: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
    ready: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["HealthResponse"];
                };
            };
            503: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ErrorEnvelope"];
                };
            };
        };
    };
}
