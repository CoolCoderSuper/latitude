import { ArrowLeft } from 'lucide-react-native';
import { useEvent } from 'expo';
import { Image } from 'expo-image';
import { VideoView, useVideoPlayer } from 'expo-video';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Text,
  View,
} from 'react-native';
import WebView from 'react-native-webview';

import { absoluteUrl, authHeaders } from '../../api';
import { IconButton, ScreenHeader } from '../../components/ui';
import type { ViewerState } from '../../navigationTypes';
import { useTheme } from '../../theme';
import type { AppStyles, ThemeColors, ThemeMode } from '../../theme';
import { prependDeviceHostname } from '../../utils/headers';
import {
  isImageMediaType,
  isVideoMediaType,
  normalizeMediaType,
} from './media';

export function DeploymentViewer({
  baseUrl,
  deviceHostname,
  onBack,
  token,
  viewer,
}: {
  baseUrl: string;
  deviceHostname?: string;
  onBack: () => void;
  token: string;
  viewer: ViewerState;
}) {
  const { colors, mode, styles } = useTheme();
  const uri = absoluteUrl(baseUrl, viewer.href);
  const mediaType = normalizeMediaType(viewer.mediaType);

  if (isVideoMediaType(mediaType)) {
    return (
      <NativeVideoViewer
        mediaUri={rawMediaUrl(uri)}
        deviceHostname={deviceHostname}
        title={viewer.title}
        token={token}
        uri={uri}
        onBack={onBack}
      />
    );
  }

  if (isImageMediaType(mediaType)) {
    return (
      <NativeImageViewer
        mediaUri={rawMediaUrl(uri)}
        deviceHostname={deviceHostname}
        title={viewer.title}
        token={token}
        uri={uri}
        onBack={onBack}
      />
    );
  }

  return (
    <WebDeploymentViewer
      colors={colors}
      deviceHostname={deviceHostname}
      mode={mode}
      onBack={onBack}
      styles={styles}
      token={token}
      uri={uri}
      viewer={viewer}
    />
  );
}

function WebDeploymentViewer({
  colors,
  deviceHostname,
  mode,
  onBack,
  styles,
  token,
  uri,
  viewer,
}: {
  colors: ThemeColors;
  deviceHostname?: string;
  mode: ThemeMode;
  onBack: () => void;
  styles: AppStyles;
  token: string;
  uri: string;
  viewer: ViewerState;
}) {
  const webViewRef = useRef<WebView>(null);
  const shouldThemePage = viewer.kind === 'page';
  const themeScript = useMemo(
    () => (shouldThemePage ? deploymentThemeScript(mode, colors) : 'true;'),
    [colors, mode, shouldThemePage],
  );

  useEffect(() => {
    webViewRef.current?.injectJavaScript(themeScript);
  }, [themeScript]);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={prependDeviceHostname(uri, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={viewer.title}
      />
      <WebView
        ref={webViewRef}
        injectedJavaScript={themeScript}
        injectedJavaScriptBeforeContentLoaded={themeScript}
        javaScriptEnabled
        originWhitelist={['http://*', 'https://*']}
        sharedCookiesEnabled
        source={{
          uri,
          headers: {
            ...authHeaders(token),
            ...(shouldThemePage ? { 'X-Latitude-Theme': mode } : {}),
          },
        }}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}

function NativeVideoViewer({
  deviceHostname,
  mediaUri,
  onBack,
  title,
  token,
  uri,
}: {
  deviceHostname?: string;
  mediaUri: string;
  onBack: () => void;
  title: string;
  token: string;
  uri: string;
}) {
  const { colors, styles } = useTheme();
  const source = useMemo(
    () => ({
      uri: mediaUri,
      contentType: 'progressive' as const,
      headers: authHeaders(token),
      metadata: {
        title,
      },
      useCaching: false,
    }),
    [mediaUri, title, token],
  );
  const player = useVideoPlayer(source, (nextPlayer) => {
    nextPlayer.loop = false;
    nextPlayer.play();
  });
  const statusChange = useEvent(player, 'statusChange', {
    status: player.status,
  });
  const playerError =
    statusChange.status === 'error'
      ? statusChange.error?.message ?? 'Could not load this video.'
      : null;

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={prependDeviceHostname(uri, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={title}
      />
      <View style={styles.videoViewer}>
        <VideoView
          allowsPictureInPicture
          contentFit="contain"
          fullscreenOptions={{ enable: true }}
          nativeControls
          player={player}
          style={styles.videoPlayer}
        />
        {playerError && (
          <View style={styles.mediaStatusOverlay}>
            <Text style={styles.imageErrorText}>{playerError}</Text>
          </View>
        )}
      </View>
    </View>
  );
}

function NativeImageViewer({
  deviceHostname,
  mediaUri,
  onBack,
  title,
  token,
  uri,
}: {
  deviceHostname?: string;
  mediaUri: string;
  onBack: () => void;
  title: string;
  token: string;
  uri: string;
}) {
  const { colors, styles } = useTheme();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const source = useMemo(
    () => ({
      cacheKey: mediaUri,
      uri: mediaUri,
      headers: authHeaders(token),
    }),
    [mediaUri, token],
  );

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={prependDeviceHostname(uri, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={title}
      />
      <View style={styles.imageViewer}>
        <Image
          accessibilityLabel={title}
          cachePolicy="memory-disk"
          contentFit="contain"
          onError={(event) => {
            setError(event.error || 'Could not load this image.');
            setLoading(false);
          }}
          onLoad={() => {
            setError(null);
          }}
          onLoadEnd={() => {
            setLoading(false);
          }}
          onLoadStart={() => {
            setError(null);
            setLoading(true);
          }}
          source={source}
          style={styles.nativeImage}
        />
        {loading && (
          <View style={styles.mediaStatusOverlay}>
            <ActivityIndicator color={colors.onAccent} />
            <Text style={styles.imageStatusText}>Loading image</Text>
          </View>
        )}
        {error && (
          <View style={styles.mediaStatusOverlay}>
            <Text style={styles.imageErrorText}>{error}</Text>
          </View>
        )}
      </View>
    </View>
  );
}

function rawMediaUrl(uri: string): string {
  const url = new URL(uri);
  url.searchParams.set('raw', '1');
  return url.toString();
}

function deploymentThemeScript(mode: ThemeMode, colors: ThemeColors): string {
  const theme = {
    mode,
    variables: {
      '--latitude-page-bg': colors.background,
      '--latitude-page-text': colors.text,
      '--latitude-page-heading': colors.text,
      '--latitude-page-muted': colors.softText,
      '--latitude-page-accent': colors.accent,
      '--latitude-page-inline-code-bg': colors.panel,
      '--latitude-page-code-bg': colors.codeBg,
      '--latitude-page-code-text': colors.codeText,
      '--latitude-page-border': colors.border,
    },
  };

  return `
(function() {
  var theme = ${JSON.stringify(theme)};
  var applyTheme = function() {
    var root = document.documentElement;
    if (!root) {
      return;
    }

    root.dataset.latitudeTheme = theme.mode;
    root.style.colorScheme = theme.mode;

    Object.keys(theme.variables).forEach(function(name) {
      root.style.setProperty(name, theme.variables[name]);
    });
  };

  applyTheme();
  document.addEventListener('DOMContentLoaded', applyTheme);
})();
true;
`;
}
