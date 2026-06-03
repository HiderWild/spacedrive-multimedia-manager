import { useNormalizedQuery } from "@sd/ts-client";
import { useTranslation } from "react-i18next";
import { SpaceItem } from "./SpaceItem";
import { GroupHeader } from "./GroupHeader";

interface LocationsGroupProps {
  isCollapsed: boolean;
  onToggle: () => void;
  sortableAttributes?: any;
  sortableListeners?: any;
}

export function LocationsGroup({
  isCollapsed,
  onToggle,
  sortableAttributes,
  sortableListeners,
}: LocationsGroupProps) {
  const { t } = useTranslation('sidebar');
  const { data: locationsData } = useNormalizedQuery({
    query: "locations.list",
    input: null, // Unit struct serializes as null, not {}
    resourceType: "location",
  });

  const locations = locationsData?.locations ?? [];

  return (
    <div>
      <GroupHeader
        label={t('sections.locations')}
        isCollapsed={isCollapsed}
        onToggle={onToggle}
        sortableAttributes={sortableAttributes}
        sortableListeners={sortableListeners}
      />

      {/* Items */}
      {!isCollapsed && (
        <div className="space-y-0.5">
          {locations.map((location: any, index: number) => (
            <SpaceItem
              key={location.id}
              item={location}
              allowInsertion={false}
              isLastItem={index === locations.length - 1}
            />
          ))}
        </div>
      )}
    </div>
  );
}
