// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { type InfiniteData, infiniteQueryOptions, queryOptions, type UseMutationOptions } from '@tanstack/react-query';
import { client } from '../client.gen';
import { createAdminGroup, createAdminToken, createAdminUser, deleteAdminGroup, deleteAdminToken, deleteAdminUser, getAdminGroup, getAdminToken, getAdminUser, getManagementSession, listAdminGroups, listAdminTokens, listAdminUsers, loginManagementSession, type Options, updateAdminGroup, updateAdminToken, updateAdminUser } from '../sdk.gen';
import type { CreateAdminGroupData, CreateAdminGroupError, CreateAdminGroupResponse, CreateAdminTokenData, CreateAdminTokenError, CreateAdminTokenResponse, CreateAdminUserData, CreateAdminUserError, CreateAdminUserResponse, DeleteAdminGroupData, DeleteAdminGroupError, DeleteAdminGroupResponse, DeleteAdminTokenData, DeleteAdminTokenError, DeleteAdminTokenResponse, DeleteAdminUserData, DeleteAdminUserError, DeleteAdminUserResponse, GetAdminGroupData, GetAdminGroupError, GetAdminGroupResponse, GetAdminTokenData, GetAdminTokenError, GetAdminTokenResponse, GetAdminUserData, GetAdminUserError, GetAdminUserResponse, GetManagementSessionData, GetManagementSessionError, GetManagementSessionResponse, ListAdminGroupsData, ListAdminGroupsError, ListAdminGroupsResponse, ListAdminTokensData, ListAdminTokensError, ListAdminTokensResponse, ListAdminUsersData, ListAdminUsersError, ListAdminUsersResponse, LoginManagementSessionData, LoginManagementSessionError, LoginManagementSessionResponse, UpdateAdminGroupData, UpdateAdminGroupError, UpdateAdminGroupResponse, UpdateAdminTokenData, UpdateAdminTokenError, UpdateAdminTokenResponse, UpdateAdminUserData, UpdateAdminUserError, UpdateAdminUserResponse } from '../types.gen';
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
    const opts = infiniteQueryOptions<ListAdminUsersResponse, ListAdminUsersError, InfiniteData<ListAdminUsersResponse>, QueryKey<Options<ListAdminUsersData>>, number | Pick<QueryKey<Options<ListAdminUsersData>>[0], 'body' | 'headers' | 'path' | 'query'>>({
        queryFn: async ({ pageParam, queryKey, signal }) => {
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
    const opts = infiniteQueryOptions<ListAdminGroupsResponse, ListAdminGroupsError, InfiniteData<ListAdminGroupsResponse>, QueryKey<Options<ListAdminGroupsData>>, number | Pick<QueryKey<Options<ListAdminGroupsData>>[0], 'body' | 'headers' | 'path' | 'query'>>({
        queryFn: async ({ pageParam, queryKey, signal }) => {
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
    const opts = infiniteQueryOptions<ListAdminTokensResponse, ListAdminTokensError, InfiniteData<ListAdminTokensResponse>, QueryKey<Options<ListAdminTokensData>>, number | Pick<QueryKey<Options<ListAdminTokensData>>[0], 'body' | 'headers' | 'path' | 'query'>>({
        queryFn: async ({ pageParam, queryKey, signal }) => {
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
