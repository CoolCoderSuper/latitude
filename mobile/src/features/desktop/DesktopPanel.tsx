import { useEffect, useMemo, useRef } from 'react';
import { AppState, View } from 'react-native';
import WebView from 'react-native-webview';

import { normalizeBaseUrl } from '../../api';
import { useTheme } from '../../theme';
import type { RootDesktopLink, SessionRecord } from '../../types';
import { desktopDocument } from './desktopDocument';

export function RootDesktopPanel({
  rootDesktop,
  session,
}: {
  rootDesktop: RootDesktopLink;
  session: SessionRecord;
}) {
  const { styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const desktopUrl = useMemo(
    () => desktopWebSocketUrl(session.baseUrl, rootDesktop.href, session.token),
    [rootDesktop.href, session.baseUrl, session.token],
  );
  const desktopHtml = useMemo(
    () =>
      desktopDocument(
        rootDesktop.label,
        desktopUrl,
        rootDesktop.view_only,
        rootDesktop.screens ?? [],
      ),
    [desktopUrl, rootDesktop.label, rootDesktop.screens, rootDesktop.view_only],
  );

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        webViewRef.current?.injectJavaScript(
          'window.latitudeReconnect && window.latitudeReconnect(true); true;',
        );
      }
    });
    return () => subscription.remove();
  }, []);

  return (
    <View style={styles.desktopPanel}>
      <WebView
        ref={webViewRef}
        domStorageEnabled
        javaScriptEnabled
        mixedContentMode="always"
        originWhitelist={['*']}
        bounces={false}
        scrollEnabled={false}
        setSupportMultipleWindows={false}
        showsHorizontalScrollIndicator={false}
        showsVerticalScrollIndicator={false}
        source={{ html: desktopHtml, baseUrl: normalizeBaseUrl(session.baseUrl) }}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}

function desktopWebSocketUrl(
  baseUrl: string,
  desktopHref: string,
  token: string,
): string {
  const cleanHref = desktopHref.replace(/\/+$/, '');
  const url = new URL(`${cleanHref}/ws`, `${normalizeBaseUrl(baseUrl)}/`);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  return url.toString();
}
