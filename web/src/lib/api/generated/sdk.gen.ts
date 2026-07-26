// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。

import type { Client, ClientMeta, Options as Options2, TDataShape } from './client';
export type Options<TData extends TDataShape = TDataShape, ThrowOnError extends boolean = boolean, TResponse = unknown> = Options2<TData, ThrowOnError, TResponse> & {
    client?: Client;
    meta?: keyof ClientMeta extends never ? Record<string, unknown> : ClientMeta;
};
