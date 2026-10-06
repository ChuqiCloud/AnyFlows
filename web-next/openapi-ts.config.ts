import { defineConfig } from '@hey-api/openapi-ts'

// 生成目录只接收后端契约产物，业务代码不得直接修改其中的文件。
export default defineConfig({
  input: './openapi/openapi.json',
  output: {
    path: './src/lib/api/generated',
    clean: true,
    header: ['// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。'],
  },
  plugins: [
    {
      name: '@hey-api/typescript',
      comments: false,
    },
    {
      name: '@hey-api/client-fetch',
      throwOnError: true,
      runtimeConfigPath: './src/lib/api/runtime-config.ts',
    },
    {
      name: '@hey-api/sdk',
      comments: false,
    },
    {
      name: '@tanstack/react-query',
      comments: false,
    },
  ],
})
