import { ArrowLeft } from 'lucide-react-native';
import { View } from 'react-native';

import { IconButton, ScreenHeader } from '../components/ui';
import { RootDesktopPanel } from '../features/desktop/DesktopPanel';
import { useTheme } from '../theme';
import type { RootDesktopLink, SessionRecord } from '../types';
import { appendDeviceHostname } from '../utils/headers';

export function RootDesktopScreen({
  deviceHostname,
  onBack,
  rootDesktop,
  session,
}: {
  deviceHostname?: string;
  onBack: () => void;
  rootDesktop: RootDesktopLink;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={appendDeviceHostname(rootDesktop.description, deviceHostname)}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={rootDesktop.label}
      />
      <RootDesktopPanel rootDesktop={rootDesktop} session={session} />
    </View>
  );
}
