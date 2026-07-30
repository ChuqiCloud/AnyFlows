// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { type InfiniteData, infiniteQueryOptions, queryOptions, type UseMutationOptions } from '@tanstack/react-query';
import { client } from '../client.gen';
import { createAdminChannel, createAdminCredential, createAdminGroup, createAdminToken, createAdminUser, createPlaygroundShare, createUserToken, deleteAdminChannel, deleteAdminCredential, deleteAdminGroup, deleteAdminToken, deleteAdminUser, deletePlaygroundConversation, deleteUserToken, getAdminAuthenticationSettings, getAdminChannel, getAdminCredential, getAdminDashboard, getAdminEmailSettings, getAdminGroup, getAdminSiteSettings, getAdminToken, getAdminUser, getInitialSetupStatus, getManagementSession, getPlaygroundConversation, getPlaygroundShare, getPublicSiteSettings, getRegistrationStatus, getUserToken, initializeAdminSetup, listAdminChannels, listAdminCredentials, listAdminGroups, listAdminTokens, listAdminUsageLogs, listAdminUsers, listModels, listPlaygroundConversations, listUserTokens, loginManagementSession, type Options, probeAdminChannel, registerUser, revokePlaygroundShare, savePlaygroundConversation, sendAdminEmailTest, updateAdminAuthenticationSettings, updateAdminChannel, updateAdminCredential, updateAdminEmailSettings, updateAdminGroup, updateAdminSiteSettings, updateAdminToken, updateAdminUser, updateUserToken } from '../sdk.gen';
import type { CreateAdminChannelData, CreateAdminChannelError, CreateAdminChannelResponse, CreateAdminCredentialData, CreateAdminCredentialError, CreateAdminCredentialResponse, CreateAdminGroupData, CreateAdminGroupError, CreateAdminGroupResponse, CreateAdminTokenData, CreateAdminTokenError, CreateAdminTokenResponse, CreateAdminUserData, CreateAdminUserError, CreateAdminUserResponse, CreatePlaygroundShareData, CreatePlaygroundShareError, CreatePlaygroundShareResponse, CreateUserTokenData, CreateUserTokenError, CreateUserTokenResponse, DeleteAdminChannelData, DeleteAdminChannelError, DeleteAdminChannelResponse, DeleteAdminCredentialData, DeleteAdminCredentialError, DeleteAdminCredentialResponse, DeleteAdminGroupData, DeleteAdminGroupError, DeleteAdminGroupResponse, DeleteAdminTokenData, DeleteAdminTokenError, DeleteAdminTokenResponse, DeleteAdminUserData, DeleteAdminUserError, DeleteAdminUserResponse, DeletePlaygroundConversationData, DeletePlaygroundConversationError, DeletePlaygroundConversationResponse, DeleteUserTokenData, DeleteUserTokenError, DeleteUserTokenResponse, GetAdminAuthenticationSettingsData, GetAdminAuthenticationSettingsError, GetAdminAuthenticationSettingsResponse, GetAdminChannelData, GetAdminChannelError, GetAdminChannelResponse, GetAdminCredentialData, GetAdminCredentialError, GetAdminCredentialResponse, GetAdminDashboardData, GetAdminDashboardError, GetAdminDashboardResponse, GetAdminEmailSettingsData, GetAdminEmailSettingsError, GetAdminEmailSettingsResponse, GetAdminGroupData, GetAdminGroupError, GetAdminGroupResponse, GetAdminSiteSettingsData, GetAdminSiteSettingsError, GetAdminSiteSettingsResponse, GetAdminTokenData, GetAdminTokenError, GetAdminTokenResponse, GetAdminUserData, GetAdminUserError, GetAdminUserResponse, GetInitialSetupStatusData, GetInitialSetupStatusError, GetInitialSetupStatusResponse, GetManagementSessionData, GetManagementSessionError, GetManagementSessionResponse, GetPlaygroundConversationData, GetPlaygroundConversationError, GetPlaygroundConversationResponse, GetPlaygroundShareData, GetPlaygroundShareError, GetPlaygroundShareResponse, GetPublicSiteSettingsData, GetPublicSiteSettingsError, GetPublicSiteSettingsResponse, GetRegistrationStatusData, GetRegistrationStatusError, GetRegistrationStatusResponse, GetUserTokenData, GetUserTokenError, GetUserTokenResponse, InitializeAdminSetupData, InitializeAdminSetupError, InitializeAdminSetupResponse, ListAdminChannelsData, ListAdminChannelsError, ListAdminChannelsResponse, ListAdminCredentialsData, ListAdminCredentialsError, ListAdminCredentialsResponse, ListAdminGroupsData, ListAdminGroupsError, ListAdminGroupsResponse, ListAdminTokensData, ListAdminTokensError, ListAdminTokensResponse, ListAdminUsageLogsData, ListAdminUsageLogsError, ListAdminUsageLogsResponse, ListAdminUsersData, ListAdminUsersError, ListAdminUsersResponse, ListModelsData, ListModelsError, ListModelsResponse, ListPlaygroundConversationsData, ListPlaygroundConversationsError, ListPlaygroundConversationsResponse, ListUserTokensData, ListUserTokensError, ListUserTokensResponse, LoginManagementSessionData, LoginManagementSessionError, LoginManagementSessionResponse, ProbeAdminChannelData, ProbeAdminChannelError, ProbeAdminChannelResponse, RegisterUserData, RegisterUserError, RegisterUserResponse, RevokePlaygroundShareData, RevokePlaygroundShareError, RevokePlaygroundShareResponse, SavePlaygroundConversationData, SavePlaygroundConversationError, SavePlaygroundConversationResponse, SendAdminEmailTestData, SendAdminEmailTestError, SendAdminEmailTestResponse, UpdateAdminAuthenticationSettingsData, UpdateAdminAuthenticationSettingsError, UpdateAdminAuthenticationSettingsResponse, UpdateAdminChannelData, UpdateAdminChannelError, UpdateAdminChannelResponse, UpdateAdminCredentialData, UpdateAdminCredentialError, UpdateAdminCredentialResponse, UpdateAdminEmailSettingsData, UpdateAdminEmailSettingsError, UpdateAdminEmailSettingsResponse, UpdateAdminGroupData, UpdateAdminGroupError, UpdateAdminGroupResponse, UpdateAdminSiteSettingsData, UpdateAdminSiteSettingsError, UpdateAdminSiteSettingsResponse, UpdateAdminTokenData, UpdateAdminTokenError, UpdateAdminTokenResponse, UpdateAdminUserData, UpdateAdminUserError, UpdateAdminUserResponse, UpdateUserTokenData, UpdateUserTokenError, UpdateUserTokenResponse } from '../types.gen';
export const loginManagementSessionMutation = (options?: Partial<Options<LoginManagementSessionData>>): UseMutationOptions<LoginManagementSessionResponse, LoginManagementSessionError, Options<LoginManagementSessionData>> => {
    const mutationOptions: UseMutationOptions<LoginManagementSessionResponse, LoginManagementSessionError, Options<LoginManagementSessionData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await loginManagementSession({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export type QueryKey<TOptions extends Options> = [
    Pick<TOptions, 'baseUrl' | 'body' | 'headers' | 'path' | 'query'> & {
        _id: string;
        _infinite?: boolean;
        tags?: ReadonlyArray<string>;
    }
];
const createQueryKey = <TOptions extends Options>(id: string, options?: TOptions, infinite?: boolean, tags?: ReadonlyArray<string>): [
    QueryKey<TOptions>[0]
] => {
    const params: QueryKey<TOptions>[0] = { _id: id, baseUrl: options?.baseUrl || (options?.client ?? client).getConfig().baseUrl } as QueryKey<TOptions>[0];
    if (infinite) {
        params._infinite = infinite;
    }
    if (tags) {
        params.tags = tags;
    }
    if (options?.body) {
        params.body = options.body;
    }
    if (options?.headers) {
        params.headers = options.headers;
    }
    if (options?.path) {
        params.path = options.path;
    }
    if (options?.query) {
        params.query = options.query;
    }
    return [params];
};
export const getManagementSessionQueryKey = (options?: Options<GetManagementSessionData>) => createQueryKey('getManagementSession', options);
export const getManagementSessionOptions = (options?: Options<GetManagementSessionData>) => queryOptions<GetManagementSessionResponse, GetManagementSessionError, GetManagementSessionResponse, ReturnType<typeof getManagementSessionQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getManagementSession({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getManagementSessionQueryKey(options)
});
export const getInitialSetupStatusQueryKey = (options?: Options<GetInitialSetupStatusData>) => createQueryKey('getInitialSetupStatus', options);
export const getInitialSetupStatusOptions = (options?: Options<GetInitialSetupStatusData>) => queryOptions<GetInitialSetupStatusResponse, GetInitialSetupStatusError, GetInitialSetupStatusResponse, ReturnType<typeof getInitialSetupStatusQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getInitialSetupStatus({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getInitialSetupStatusQueryKey(options)
});
export const initializeAdminSetupMutation = (options?: Partial<Options<InitializeAdminSetupData>>): UseMutationOptions<InitializeAdminSetupResponse, InitializeAdminSetupError, Options<InitializeAdminSetupData>> => {
    const mutationOptions: UseMutationOptions<InitializeAdminSetupResponse, InitializeAdminSetupError, Options<InitializeAdminSetupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await initializeAdminSetup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getRegistrationStatusQueryKey = (options?: Options<GetRegistrationStatusData>) => createQueryKey('getRegistrationStatus', options);
export const getRegistrationStatusOptions = (options?: Options<GetRegistrationStatusData>) => queryOptions<GetRegistrationStatusResponse, GetRegistrationStatusError, GetRegistrationStatusResponse, ReturnType<typeof getRegistrationStatusQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getRegistrationStatus({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getRegistrationStatusQueryKey(options)
});
export const registerUserMutation = (options?: Partial<Options<RegisterUserData>>): UseMutationOptions<RegisterUserResponse, RegisterUserError, Options<RegisterUserData>> => {
    const mutationOptions: UseMutationOptions<RegisterUserResponse, RegisterUserError, Options<RegisterUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await registerUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminAuthenticationSettingsQueryKey = (options?: Options<GetAdminAuthenticationSettingsData>) => createQueryKey('getAdminAuthenticationSettings', options);
export const getAdminAuthenticationSettingsOptions = (options?: Options<GetAdminAuthenticationSettingsData>) => queryOptions<GetAdminAuthenticationSettingsResponse, GetAdminAuthenticationSettingsError, GetAdminAuthenticationSettingsResponse, ReturnType<typeof getAdminAuthenticationSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminAuthenticationSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminAuthenticationSettingsQueryKey(options)
});
export const updateAdminAuthenticationSettingsMutation = (options?: Partial<Options<UpdateAdminAuthenticationSettingsData>>): UseMutationOptions<UpdateAdminAuthenticationSettingsResponse, UpdateAdminAuthenticationSettingsError, Options<UpdateAdminAuthenticationSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminAuthenticationSettingsResponse, UpdateAdminAuthenticationSettingsError, Options<UpdateAdminAuthenticationSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminAuthenticationSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminEmailSettingsQueryKey = (options?: Options<GetAdminEmailSettingsData>) => createQueryKey('getAdminEmailSettings', options);
export const getAdminEmailSettingsOptions = (options?: Options<GetAdminEmailSettingsData>) => queryOptions<GetAdminEmailSettingsResponse, GetAdminEmailSettingsError, GetAdminEmailSettingsResponse, ReturnType<typeof getAdminEmailSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminEmailSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminEmailSettingsQueryKey(options)
});
export const updateAdminEmailSettingsMutation = (options?: Partial<Options<UpdateAdminEmailSettingsData>>): UseMutationOptions<UpdateAdminEmailSettingsResponse, UpdateAdminEmailSettingsError, Options<UpdateAdminEmailSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminEmailSettingsResponse, UpdateAdminEmailSettingsError, Options<UpdateAdminEmailSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminEmailSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const sendAdminEmailTestMutation = (options?: Partial<Options<SendAdminEmailTestData>>): UseMutationOptions<SendAdminEmailTestResponse, SendAdminEmailTestError, Options<SendAdminEmailTestData>> => {
    const mutationOptions: UseMutationOptions<SendAdminEmailTestResponse, SendAdminEmailTestError, Options<SendAdminEmailTestData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await sendAdminEmailTest({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPublicSiteSettingsQueryKey = (options?: Options<GetPublicSiteSettingsData>) => createQueryKey('getPublicSiteSettings', options);
export const getPublicSiteSettingsOptions = (options?: Options<GetPublicSiteSettingsData>) => queryOptions<GetPublicSiteSettingsResponse, GetPublicSiteSettingsError, GetPublicSiteSettingsResponse, ReturnType<typeof getPublicSiteSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPublicSiteSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPublicSiteSettingsQueryKey(options)
});
export const getAdminSiteSettingsQueryKey = (options?: Options<GetAdminSiteSettingsData>) => createQueryKey('getAdminSiteSettings', options);
export const getAdminSiteSettingsOptions = (options?: Options<GetAdminSiteSettingsData>) => queryOptions<GetAdminSiteSettingsResponse, GetAdminSiteSettingsError, GetAdminSiteSettingsResponse, ReturnType<typeof getAdminSiteSettingsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminSiteSettings({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminSiteSettingsQueryKey(options)
});
export const updateAdminSiteSettingsMutation = (options?: Partial<Options<UpdateAdminSiteSettingsData>>): UseMutationOptions<UpdateAdminSiteSettingsResponse, UpdateAdminSiteSettingsError, Options<UpdateAdminSiteSettingsData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminSiteSettingsResponse, UpdateAdminSiteSettingsError, Options<UpdateAdminSiteSettingsData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminSiteSettings({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminDashboardQueryKey = (options?: Options<GetAdminDashboardData>) => createQueryKey('getAdminDashboard', options);
export const getAdminDashboardOptions = (options?: Options<GetAdminDashboardData>) => queryOptions<GetAdminDashboardResponse, GetAdminDashboardError, GetAdminDashboardResponse, ReturnType<typeof getAdminDashboardQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminDashboard({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminDashboardQueryKey(options)
});
export const listAdminUsersQueryKey = (options?: Options<ListAdminUsersData>) => createQueryKey('listAdminUsers', options);
export const listAdminUsersOptions = (options?: Options<ListAdminUsersData>) => queryOptions<ListAdminUsersResponse, ListAdminUsersError, ListAdminUsersResponse, ReturnType<typeof listAdminUsersQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminUsers({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminUsersQueryKey(options)
});
const createInfiniteParams = <K extends Pick<QueryKey<Options>[0], 'body' | 'headers' | 'path' | 'query'>>(queryKey: QueryKey<Options>, page: K) => {
    const params = { ...queryKey[0] };
    if (page.body) {
        params.body = {
            ...queryKey[0].body as any,
            ...page.body as any
        };
    }
    if (page.headers) {
        params.headers = {
            ...queryKey[0].headers,
            ...page.headers
        };
    }
    if (page.path) {
        params.path = {
            ...queryKey[0].path as any,
            ...page.path as any
        };
    }
    if (page.query) {
        params.query = {
            ...queryKey[0].query as any,
            ...page.query as any
        };
    }
    return params as unknown as typeof page;
};
export const listAdminUsersInfiniteQueryKey = (options?: Options<ListAdminUsersData>): QueryKey<Options<ListAdminUsersData>> => createQueryKey('listAdminUsers', options, true);
export const listAdminUsersInfiniteOptions = (options?: Options<ListAdminUsersData>) => {
    const opts = infiniteQueryOptions<ListAdminUsersResponse, ListAdminUsersError, InfiniteData<ListAdminUsersResponse>, QueryKey<Options<ListAdminUsersData>>, number | Pick<QueryKey<Options<ListAdminUsersData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminUsersData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminUsers({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminUsersInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminUserMutation = (options?: Partial<Options<CreateAdminUserData>>): UseMutationOptions<CreateAdminUserResponse, CreateAdminUserError, Options<CreateAdminUserData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminUserResponse, CreateAdminUserError, Options<CreateAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminUserMutation = (options?: Partial<Options<DeleteAdminUserData>>): UseMutationOptions<DeleteAdminUserResponse, DeleteAdminUserError, Options<DeleteAdminUserData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminUserResponse, DeleteAdminUserError, Options<DeleteAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminUserQueryKey = (options: Options<GetAdminUserData>) => createQueryKey('getAdminUser', options);
export const getAdminUserOptions = (options: Options<GetAdminUserData>) => queryOptions<GetAdminUserResponse, GetAdminUserError, GetAdminUserResponse, ReturnType<typeof getAdminUserQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminUser({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminUserQueryKey(options)
});
export const updateAdminUserMutation = (options?: Partial<Options<UpdateAdminUserData>>): UseMutationOptions<UpdateAdminUserResponse, UpdateAdminUserError, Options<UpdateAdminUserData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminUserResponse, UpdateAdminUserError, Options<UpdateAdminUserData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminUser({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminGroupsQueryKey = (options?: Options<ListAdminGroupsData>) => createQueryKey('listAdminGroups', options);
export const listAdminGroupsOptions = (options?: Options<ListAdminGroupsData>) => queryOptions<ListAdminGroupsResponse, ListAdminGroupsError, ListAdminGroupsResponse, ReturnType<typeof listAdminGroupsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminGroups({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminGroupsQueryKey(options)
});
export const listAdminGroupsInfiniteQueryKey = (options?: Options<ListAdminGroupsData>): QueryKey<Options<ListAdminGroupsData>> => createQueryKey('listAdminGroups', options, true);
export const listAdminGroupsInfiniteOptions = (options?: Options<ListAdminGroupsData>) => {
    const opts = infiniteQueryOptions<ListAdminGroupsResponse, ListAdminGroupsError, InfiniteData<ListAdminGroupsResponse>, QueryKey<Options<ListAdminGroupsData>>, number | Pick<QueryKey<Options<ListAdminGroupsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminGroupsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminGroups({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminGroupsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminGroupMutation = (options?: Partial<Options<CreateAdminGroupData>>): UseMutationOptions<CreateAdminGroupResponse, CreateAdminGroupError, Options<CreateAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminGroupResponse, CreateAdminGroupError, Options<CreateAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminGroupMutation = (options?: Partial<Options<DeleteAdminGroupData>>): UseMutationOptions<DeleteAdminGroupResponse, DeleteAdminGroupError, Options<DeleteAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminGroupResponse, DeleteAdminGroupError, Options<DeleteAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminGroupQueryKey = (options: Options<GetAdminGroupData>) => createQueryKey('getAdminGroup', options);
export const getAdminGroupOptions = (options: Options<GetAdminGroupData>) => queryOptions<GetAdminGroupResponse, GetAdminGroupError, GetAdminGroupResponse, ReturnType<typeof getAdminGroupQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminGroup({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminGroupQueryKey(options)
});
export const updateAdminGroupMutation = (options?: Partial<Options<UpdateAdminGroupData>>): UseMutationOptions<UpdateAdminGroupResponse, UpdateAdminGroupError, Options<UpdateAdminGroupData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminGroupResponse, UpdateAdminGroupError, Options<UpdateAdminGroupData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminGroup({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listModelsQueryKey = (options?: Options<ListModelsData>) => createQueryKey('listModels', options);
export const listModelsOptions = (options?: Options<ListModelsData>) => queryOptions<ListModelsResponse, ListModelsError, ListModelsResponse, ReturnType<typeof listModelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listModels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listModelsQueryKey(options)
});
export const listModelsInfiniteQueryKey = (options?: Options<ListModelsData>): QueryKey<Options<ListModelsData>> => createQueryKey('listModels', options, true);
export const listModelsInfiniteOptions = (options?: Options<ListModelsData>) => {
    const opts = infiniteQueryOptions<ListModelsResponse, ListModelsError, InfiniteData<ListModelsResponse>, QueryKey<Options<ListModelsData>>, string | Pick<QueryKey<Options<ListModelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListModelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listModels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listModelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createPlaygroundShareMutation = (options?: Partial<Options<CreatePlaygroundShareData>>): UseMutationOptions<CreatePlaygroundShareResponse, CreatePlaygroundShareError, Options<CreatePlaygroundShareData>> => {
    const mutationOptions: UseMutationOptions<CreatePlaygroundShareResponse, CreatePlaygroundShareError, Options<CreatePlaygroundShareData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createPlaygroundShare({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const revokePlaygroundShareMutation = (options?: Partial<Options<RevokePlaygroundShareData>>): UseMutationOptions<RevokePlaygroundShareResponse, RevokePlaygroundShareError, Options<RevokePlaygroundShareData>> => {
    const mutationOptions: UseMutationOptions<RevokePlaygroundShareResponse, RevokePlaygroundShareError, Options<RevokePlaygroundShareData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await revokePlaygroundShare({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPlaygroundShareQueryKey = (options: Options<GetPlaygroundShareData>) => createQueryKey('getPlaygroundShare', options);
export const getPlaygroundShareOptions = (options: Options<GetPlaygroundShareData>) => queryOptions<GetPlaygroundShareResponse, GetPlaygroundShareError, GetPlaygroundShareResponse, ReturnType<typeof getPlaygroundShareQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPlaygroundShare({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPlaygroundShareQueryKey(options)
});
export const listPlaygroundConversationsQueryKey = (options?: Options<ListPlaygroundConversationsData>) => createQueryKey('listPlaygroundConversations', options);
export const listPlaygroundConversationsOptions = (options?: Options<ListPlaygroundConversationsData>) => queryOptions<ListPlaygroundConversationsResponse, ListPlaygroundConversationsError, ListPlaygroundConversationsResponse, ReturnType<typeof listPlaygroundConversationsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listPlaygroundConversations({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listPlaygroundConversationsQueryKey(options)
});
export const deletePlaygroundConversationMutation = (options?: Partial<Options<DeletePlaygroundConversationData>>): UseMutationOptions<DeletePlaygroundConversationResponse, DeletePlaygroundConversationError, Options<DeletePlaygroundConversationData>> => {
    const mutationOptions: UseMutationOptions<DeletePlaygroundConversationResponse, DeletePlaygroundConversationError, Options<DeletePlaygroundConversationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deletePlaygroundConversation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getPlaygroundConversationQueryKey = (options: Options<GetPlaygroundConversationData>) => createQueryKey('getPlaygroundConversation', options);
export const getPlaygroundConversationOptions = (options: Options<GetPlaygroundConversationData>) => queryOptions<GetPlaygroundConversationResponse, GetPlaygroundConversationError, GetPlaygroundConversationResponse, ReturnType<typeof getPlaygroundConversationQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getPlaygroundConversation({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getPlaygroundConversationQueryKey(options)
});
export const savePlaygroundConversationMutation = (options?: Partial<Options<SavePlaygroundConversationData>>): UseMutationOptions<SavePlaygroundConversationResponse, SavePlaygroundConversationError, Options<SavePlaygroundConversationData>> => {
    const mutationOptions: UseMutationOptions<SavePlaygroundConversationResponse, SavePlaygroundConversationError, Options<SavePlaygroundConversationData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await savePlaygroundConversation({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listUserTokensQueryKey = (options?: Options<ListUserTokensData>) => createQueryKey('listUserTokens', options);
export const listUserTokensOptions = (options?: Options<ListUserTokensData>) => queryOptions<ListUserTokensResponse, ListUserTokensError, ListUserTokensResponse, ReturnType<typeof listUserTokensQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listUserTokens({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listUserTokensQueryKey(options)
});
export const listUserTokensInfiniteQueryKey = (options?: Options<ListUserTokensData>): QueryKey<Options<ListUserTokensData>> => createQueryKey('listUserTokens', options, true);
export const listUserTokensInfiniteOptions = (options?: Options<ListUserTokensData>) => {
    const opts = infiniteQueryOptions<ListUserTokensResponse, ListUserTokensError, InfiniteData<ListUserTokensResponse>, QueryKey<Options<ListUserTokensData>>, number | Pick<QueryKey<Options<ListUserTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListUserTokensData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listUserTokens({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listUserTokensInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createUserTokenMutation = (options?: Partial<Options<CreateUserTokenData>>): UseMutationOptions<CreateUserTokenResponse, CreateUserTokenError, Options<CreateUserTokenData>> => {
    const mutationOptions: UseMutationOptions<CreateUserTokenResponse, CreateUserTokenError, Options<CreateUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteUserTokenMutation = (options?: Partial<Options<DeleteUserTokenData>>): UseMutationOptions<DeleteUserTokenResponse, DeleteUserTokenError, Options<DeleteUserTokenData>> => {
    const mutationOptions: UseMutationOptions<DeleteUserTokenResponse, DeleteUserTokenError, Options<DeleteUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getUserTokenQueryKey = (options: Options<GetUserTokenData>) => createQueryKey('getUserToken', options);
export const getUserTokenOptions = (options: Options<GetUserTokenData>) => queryOptions<GetUserTokenResponse, GetUserTokenError, GetUserTokenResponse, ReturnType<typeof getUserTokenQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getUserToken({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getUserTokenQueryKey(options)
});
export const updateUserTokenMutation = (options?: Partial<Options<UpdateUserTokenData>>): UseMutationOptions<UpdateUserTokenResponse, UpdateUserTokenError, Options<UpdateUserTokenData>> => {
    const mutationOptions: UseMutationOptions<UpdateUserTokenResponse, UpdateUserTokenError, Options<UpdateUserTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateUserToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminTokensQueryKey = (options?: Options<ListAdminTokensData>) => createQueryKey('listAdminTokens', options);
export const listAdminTokensOptions = (options?: Options<ListAdminTokensData>) => queryOptions<ListAdminTokensResponse, ListAdminTokensError, ListAdminTokensResponse, ReturnType<typeof listAdminTokensQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminTokens({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminTokensQueryKey(options)
});
export const listAdminTokensInfiniteQueryKey = (options?: Options<ListAdminTokensData>): QueryKey<Options<ListAdminTokensData>> => createQueryKey('listAdminTokens', options, true);
export const listAdminTokensInfiniteOptions = (options?: Options<ListAdminTokensData>) => {
    const opts = infiniteQueryOptions<ListAdminTokensResponse, ListAdminTokensError, InfiniteData<ListAdminTokensResponse>, QueryKey<Options<ListAdminTokensData>>, number | Pick<QueryKey<Options<ListAdminTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminTokensData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminTokens({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminTokensInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminTokenMutation = (options?: Partial<Options<CreateAdminTokenData>>): UseMutationOptions<CreateAdminTokenResponse, CreateAdminTokenError, Options<CreateAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminTokenResponse, CreateAdminTokenError, Options<CreateAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminTokenMutation = (options?: Partial<Options<DeleteAdminTokenData>>): UseMutationOptions<DeleteAdminTokenResponse, DeleteAdminTokenError, Options<DeleteAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminTokenResponse, DeleteAdminTokenError, Options<DeleteAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminTokenQueryKey = (options: Options<GetAdminTokenData>) => createQueryKey('getAdminToken', options);
export const getAdminTokenOptions = (options: Options<GetAdminTokenData>) => queryOptions<GetAdminTokenResponse, GetAdminTokenError, GetAdminTokenResponse, ReturnType<typeof getAdminTokenQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminToken({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminTokenQueryKey(options)
});
export const updateAdminTokenMutation = (options?: Partial<Options<UpdateAdminTokenData>>): UseMutationOptions<UpdateAdminTokenResponse, UpdateAdminTokenError, Options<UpdateAdminTokenData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminTokenResponse, UpdateAdminTokenError, Options<UpdateAdminTokenData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminToken({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminUsageLogsQueryKey = (options?: Options<ListAdminUsageLogsData>) => createQueryKey('listAdminUsageLogs', options);
export const listAdminUsageLogsOptions = (options?: Options<ListAdminUsageLogsData>) => queryOptions<ListAdminUsageLogsResponse, ListAdminUsageLogsError, ListAdminUsageLogsResponse, ReturnType<typeof listAdminUsageLogsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminUsageLogs({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminUsageLogsQueryKey(options)
});
export const listAdminUsageLogsInfiniteQueryKey = (options?: Options<ListAdminUsageLogsData>): QueryKey<Options<ListAdminUsageLogsData>> => createQueryKey('listAdminUsageLogs', options, true);
export const listAdminUsageLogsInfiniteOptions = (options?: Options<ListAdminUsageLogsData>) => {
    const opts = infiniteQueryOptions<ListAdminUsageLogsResponse, ListAdminUsageLogsError, InfiniteData<ListAdminUsageLogsResponse>, QueryKey<Options<ListAdminUsageLogsData>>, number | Pick<QueryKey<Options<ListAdminUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminUsageLogsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    before: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminUsageLogs({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminUsageLogsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const listAdminChannelsQueryKey = (options?: Options<ListAdminChannelsData>) => createQueryKey('listAdminChannels', options);
export const listAdminChannelsOptions = (options?: Options<ListAdminChannelsData>) => queryOptions<ListAdminChannelsResponse, ListAdminChannelsError, ListAdminChannelsResponse, ReturnType<typeof listAdminChannelsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminChannels({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminChannelsQueryKey(options)
});
export const listAdminChannelsInfiniteQueryKey = (options?: Options<ListAdminChannelsData>): QueryKey<Options<ListAdminChannelsData>> => createQueryKey('listAdminChannels', options, true);
export const listAdminChannelsInfiniteOptions = (options?: Options<ListAdminChannelsData>) => {
    const opts = infiniteQueryOptions<ListAdminChannelsResponse, ListAdminChannelsError, InfiniteData<ListAdminChannelsResponse>, QueryKey<Options<ListAdminChannelsData>>, number | Pick<QueryKey<Options<ListAdminChannelsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminChannelsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminChannels({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminChannelsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminChannelMutation = (options?: Partial<Options<CreateAdminChannelData>>): UseMutationOptions<CreateAdminChannelResponse, CreateAdminChannelError, Options<CreateAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminChannelResponse, CreateAdminChannelError, Options<CreateAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminChannelMutation = (options?: Partial<Options<DeleteAdminChannelData>>): UseMutationOptions<DeleteAdminChannelResponse, DeleteAdminChannelError, Options<DeleteAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminChannelResponse, DeleteAdminChannelError, Options<DeleteAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminChannelQueryKey = (options: Options<GetAdminChannelData>) => createQueryKey('getAdminChannel', options);
export const getAdminChannelOptions = (options: Options<GetAdminChannelData>) => queryOptions<GetAdminChannelResponse, GetAdminChannelError, GetAdminChannelResponse, ReturnType<typeof getAdminChannelQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminChannel({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminChannelQueryKey(options)
});
export const updateAdminChannelMutation = (options?: Partial<Options<UpdateAdminChannelData>>): UseMutationOptions<UpdateAdminChannelResponse, UpdateAdminChannelError, Options<UpdateAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminChannelResponse, UpdateAdminChannelError, Options<UpdateAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const probeAdminChannelMutation = (options?: Partial<Options<ProbeAdminChannelData>>): UseMutationOptions<ProbeAdminChannelResponse, ProbeAdminChannelError, Options<ProbeAdminChannelData>> => {
    const mutationOptions: UseMutationOptions<ProbeAdminChannelResponse, ProbeAdminChannelError, Options<ProbeAdminChannelData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await probeAdminChannel({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const listAdminCredentialsQueryKey = (options: Options<ListAdminCredentialsData>) => createQueryKey('listAdminCredentials', options);
export const listAdminCredentialsOptions = (options: Options<ListAdminCredentialsData>) => queryOptions<ListAdminCredentialsResponse, ListAdminCredentialsError, ListAdminCredentialsResponse, ReturnType<typeof listAdminCredentialsQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await listAdminCredentials({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: listAdminCredentialsQueryKey(options)
});
export const listAdminCredentialsInfiniteQueryKey = (options: Options<ListAdminCredentialsData>): QueryKey<Options<ListAdminCredentialsData>> => createQueryKey('listAdminCredentials', options, true);
export const listAdminCredentialsInfiniteOptions = (options: Options<ListAdminCredentialsData>) => {
    const opts = infiniteQueryOptions<ListAdminCredentialsResponse, ListAdminCredentialsError, InfiniteData<ListAdminCredentialsResponse>, QueryKey<Options<ListAdminCredentialsData>>, number | Pick<QueryKey<Options<ListAdminCredentialsData>>[0], 'body' | 'headers' | 'path' | 'query'>>(
    // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
    {
        queryFn: async ({ pageParam, queryKey, signal }) => {
            // @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。
            const page: Pick<QueryKey<Options<ListAdminCredentialsData>>[0], 'body' | 'headers' | 'path' | 'query'> = typeof pageParam === 'object' ? pageParam : {
                query: {
                    after: pageParam
                }
            };
            const params = createInfiniteParams(queryKey, page);
            const { data } = await listAdminCredentials({
                ...options,
                ...params,
                signal,
                throwOnError: true
            });
            return data;
        },
        queryKey: listAdminCredentialsInfiniteQueryKey(options)
    });
    return opts as Omit<typeof opts, 'initialData'>;
};
export const createAdminCredentialMutation = (options?: Partial<Options<CreateAdminCredentialData>>): UseMutationOptions<CreateAdminCredentialResponse, CreateAdminCredentialError, Options<CreateAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<CreateAdminCredentialResponse, CreateAdminCredentialError, Options<CreateAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await createAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const deleteAdminCredentialMutation = (options?: Partial<Options<DeleteAdminCredentialData>>): UseMutationOptions<DeleteAdminCredentialResponse, DeleteAdminCredentialError, Options<DeleteAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<DeleteAdminCredentialResponse, DeleteAdminCredentialError, Options<DeleteAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await deleteAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
export const getAdminCredentialQueryKey = (options: Options<GetAdminCredentialData>) => createQueryKey('getAdminCredential', options);
export const getAdminCredentialOptions = (options: Options<GetAdminCredentialData>) => queryOptions<GetAdminCredentialResponse, GetAdminCredentialError, GetAdminCredentialResponse, ReturnType<typeof getAdminCredentialQueryKey>>({
    queryFn: async ({ queryKey, signal }) => {
        const { data } = await getAdminCredential({
            ...options,
            ...queryKey[0],
            signal,
            throwOnError: true
        });
        return data;
    },
    queryKey: getAdminCredentialQueryKey(options)
});
export const updateAdminCredentialMutation = (options?: Partial<Options<UpdateAdminCredentialData>>): UseMutationOptions<UpdateAdminCredentialResponse, UpdateAdminCredentialError, Options<UpdateAdminCredentialData>> => {
    const mutationOptions: UseMutationOptions<UpdateAdminCredentialResponse, UpdateAdminCredentialError, Options<UpdateAdminCredentialData>> = {
        mutationFn: async (fnOptions) => {
            const { data } = await updateAdminCredential({
                ...options,
                ...fnOptions,
                throwOnError: true
            });
            return data;
        }
    };
    return mutationOptions;
};
