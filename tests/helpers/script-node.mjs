export function scriptNode() {
  return process.env.TSRX_SCRIPT_NODE ?? process.execPath;
}
