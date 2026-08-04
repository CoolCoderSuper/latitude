import { createContext, useContext, useMemo } from 'react';
import type { ReactNode } from 'react';

import { LatitudePublicApi } from '../../api';
import { useSession } from '../session/SessionContext';

const ApiContext = createContext<LatitudePublicApi | null>(null);

export function ApiProvider({ children }: { children: ReactNode }) {
  const { session } = useSession();
  const api = useMemo(
    () => new LatitudePublicApi(session?.baseUrl ?? '', session?.token),
    [session?.baseUrl, session?.token],
  );

  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>;
}

export function useLatitudeApi(): LatitudePublicApi {
  const api = useContext(ApiContext);
  if (!api) {
    throw new Error('ApiContext is missing.');
  }
  return api;
}
