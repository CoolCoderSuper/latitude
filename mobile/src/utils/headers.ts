export function appendDeviceHostname(
  label: string,
  deviceHostname?: string,
): string {
  const hostname = cleanDeviceHostname(deviceHostname);
  return hostname ? `${label} on ${hostname}` : label;
}

export function prependDeviceHostname(
  label: string,
  deviceHostname?: string,
): string {
  const hostname = cleanDeviceHostname(deviceHostname);
  return hostname ? `${hostname} - ${label}` : label;
}

function cleanDeviceHostname(deviceHostname?: string): string | null {
  const hostname = deviceHostname?.trim();
  return hostname ? hostname : null;
}
