import { ArrowLeft } from 'lucide-react-native';
import { ScrollView, View } from 'react-native';

import { IconButton, InlineNotice, ScreenHeader } from '../components/ui';
import { useTheme } from '../theme';

export function UnavailableScreen({
  message,
  onBack,
  title,
}: {
  message: string;
  onBack: () => void;
  title: string;
}) {
  const { colors, styles } = useTheme();
  return (
    <View style={styles.flex}>
      <ScreenHeader
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={title}
      />
      <ScrollView contentContainerStyle={styles.screenContent}>
        <InlineNotice text={message} tone="error" />
      </ScrollView>
    </View>
  );
}
