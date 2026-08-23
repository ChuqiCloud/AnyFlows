// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import { createClientConfig } from '../runtime-config.ts';
import { type Client, type ClientOptions, type Config, createClient, createConfig } from './client';
import type { ClientOptions as ClientOptions2 } from './types.gen';
export type CreateClientConfig<T extends ClientOptions = ClientOptions2> = (override?: Config<ClientOptions & T>) => Config<Required<ClientOptions> & T>;
export const client: Client = createClient(createClientConfig(createConfig<ClientOptions2>({ throwOnError: true })));
