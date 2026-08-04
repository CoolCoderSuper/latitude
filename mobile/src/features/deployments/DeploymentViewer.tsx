import { ArrowLeft } from 'lucide-react-native';
import { useEvent } from 'expo';
import { Image } from 'expo-image';
import { VideoView, useVideoPlayer } from 'expo-video';
import { useEffect, useMemo, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
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
import { withRawMedia } from '../../webview/urls';
import { deploymentThemeScript } from './deploymentDocument';
import { viewerStyles } from './viewerStyles';

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
        mediaUri={withRawMedia(uri)}
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
        mediaUri={withRawMedia(uri)}
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
      ? (statusChange.error?.message ?? 'Could not load this video.')
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
      <View style={viewerStyles.viewer}>
        <VideoView
          allowsPictureInPicture
          contentFit="contain"
          fullscreenOptions={{ enable: true }}
          nativeControls
          player={player}
          style={viewerStyles.media}
        />
        {playerError && (
          <View style={viewerStyles.statusOverlay}>
            <Text style={viewerStyles.errorText}>{playerError}</Text>
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
      <View style={viewerStyles.viewer}>
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
          style={viewerStyles.media}
        />
        {loading && (
          <View style={viewerStyles.statusOverlay}>
            <ActivityIndicator color={colors.onAccent} />
            <Text style={viewerStyles.statusText}>Loading image</Text>
          </View>
        )}
        {error && (
          <View style={viewerStyles.statusOverlay}>
            <Text style={viewerStyles.errorText}>{error}</Text>
          </View>
        )}
      </View>
    </View>
  );
}
