// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { queryOptions, type UseMutationOptions } from '@tanstack/react-query';
import { client } from '../client.gen';
import { getManagementSession, loginManagementSession, type Options } from '../sdk.gen';
import type { GetManagementSessionData, GetManagementSessionError, GetManagementSessionResponse, LoginManagementSessionData, LoginManagementSessionError, LoginManagementSessionResponse } from '../types.gen';
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
