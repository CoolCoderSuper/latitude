import { ArrowLeft } from 'lucide-react-native';
import { View } from 'react-native';

import type { LatitudePublicApi } from '../api';
import { IconButton, ScreenHeader } from '../components/ui';
import { RootTerminalPanel } from '../features/terminal/TerminalPanel';
import { useTheme } from '../theme';
import type { RootTerminalLink, SessionRecord } from '../types';
import { appendDeviceHostname } from '../utils/headers';

export function RootTerminalScreen({
  api,
  deviceHostname,
  onBack,
  rootTerminal,
  session,
}: {
  api: LatitudePublicApi;
  deviceHostname?: string;
  onBack: () => void;
  rootTerminal: RootTerminalLink;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname('User directory', deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={rootTerminal.label}
      />
      <RootTerminalPanel
        api={api}
        rootTerminal={rootTerminal}
        session={session}
      />
    </View>
  );
}
