/** Node 24+ JSON Lines SDK. Diagnostics belong on stderr. Return inner results;
 * this SDK wraps task/provider/credential results using the correct envelope. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
export type ObjectValue = { [key: string]: Json };
export type PluginHandler = (operation: ObjectValue) => Promise<Json> | Json;
const MAX_FRAME = 1024 * 1024;
const ERRORS = new Set(['invalid_configuration','unsupported','authentication_required','forbidden','rate_limited','temporarily_unavailable','conflict','upstream_failure','partial_success','outcome_unknown','invalid_response']);
const RESULTS: Record<string,string> = {invoke_capability:'capability',plugin_route:'capability',invoke_task:'task',invoke_provider:'provider',invoke_credential:'credential'};
export class PluginError extends Error {
  code: string;
  constructor(code: string) { super(code); this.code = ERRORS.has(code) ? code : 'invalid_response'; }
}
function object(value: Json | undefined): value is ObjectValue { return value !== null && typeof value === 'object' && !Array.isArray(value); }
export class AnyFlowsPlugin {
  manifest: ObjectValue;
  handler: PluginHandler;
  constructor(manifest: ObjectValue, handler: PluginHandler) { this.manifest = manifest; this.handler = handler; }
  async respond(frame: ObjectValue): Promise<ObjectValue> {
    const operation = frame.operation;
    if (frame.protocolVersion !== 1 || frame.pluginId !== this.manifest.pluginId || frame.pluginVersion !== this.manifest.pluginVersion ||
        typeof frame.artifactSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(frame.artifactSha256) ||
        !Number.isSafeInteger(frame.deploymentGeneration) || Number(frame.deploymentGeneration) < 1 ||
        typeof frame.requestId !== 'string' || !frame.requestId.trim() || Buffer.byteLength(frame.requestId) > 128 ||
        !Number.isSafeInteger(frame.deadlineUnixMs) || !object(operation) || typeof operation.operation !== 'string') throw new Error('invalid host envelope');
    const response: ObjectValue = {};
    for (const key of ['protocolVersion','pluginId','pluginVersion','artifactSha256','deploymentGeneration','requestId']) response[key] = frame[key];
    let result: ObjectValue;
    try {
      if (Number(frame.deadlineUnixMs) <= Date.now()) throw new PluginError('temporarily_unavailable');
      const kind = operation.operation;
      if (kind === 'manifest') result = {kind:'manifest',value:this.manifest};
      else if (kind === 'health') result = {kind:'health',value:{status:'healthy',checkedAtUnixMs:Date.now()}};
      else {
        const outputKind = RESULTS[kind];
        if (!outputKind) throw new PluginError('unsupported');
        if (typeof operation.connectionId !== 'string' || !object(operation.publicConfig) || typeof operation.capabilityId !== 'string' || !Number.isSafeInteger(operation.capabilityVersion)) throw new PluginError('invalid_configuration');
        let timer: ReturnType<typeof setTimeout> | undefined;
        let value: Json;
        try {
          value = await Promise.race([Promise.resolve(this.handler(operation)),new Promise<Json>((_,reject) => { timer = setTimeout(() => reject(new PluginError('outcome_unknown')),Math.min(Number(frame.deadlineUnixMs)-Date.now(),300_000)); })]);
        } finally { if (timer) clearTimeout(timer); }
        result = {kind:outputKind,value:outputKind === 'capability' ? {output:value} : value};
      }
    } catch (error) { result = {kind:'error',value:{code:error instanceof PluginError ? error.code : 'upstream_failure',outcome:'unknown'}}; }
    response.result = result;
    if (Buffer.byteLength(JSON.stringify(response))+1 > MAX_FRAME) throw new Error('response frame too large');
    return response;
  }
  async serve(): Promise<void> {
    let pending = Buffer.alloc(0);
    for await (const chunk of process.stdin) {
      pending = Buffer.concat([pending,Buffer.from(chunk)]);
      let end: number;
      while ((end = pending.indexOf(10)) >= 0) {
        if (end+1 > MAX_FRAME) throw new Error('host frame too large');
        const line = pending.subarray(0,end); pending = pending.subarray(end+1);
        const frame: Json = JSON.parse(line.toString('utf8'));
        if (!object(frame)) throw new Error('invalid host frame');
        const response = await this.respond(frame);
        await new Promise<void>((resolve,reject) => { process.stdout.write(JSON.stringify(response)+'\n',error => error ? reject(error) : resolve()); });
      }
      if (pending.length >= MAX_FRAME) throw new Error('host frame too large');
    }
    if (pending.length) throw new Error('unterminated host frame');
  }
}
