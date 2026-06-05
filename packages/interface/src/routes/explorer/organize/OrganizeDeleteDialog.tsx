import { useCallback } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import type { File } from "@sd/ts-client";
import { Dialog, dialogManager, useDialog, type UseDialogProps } from "@spacedrive/primitives";
import { useDeleteFilesMutation } from "../hooks/useDeleteFiles";

interface OrganizeDeleteDialogProps extends UseDialogProps {
	files: File[];
	onDeleted: (deletedPaths: string[]) => Promise<void> | void;
}

function OrganizeDeleteDialog(props: OrganizeDeleteDialogProps) {
	const { t } = useTranslation("explorer");
	const dialog = useDialog(props);
	const form = useForm();
	const { deleteFilesDirect, isPending } = useDeleteFilesMutation();

	const handleConfirm = useCallback(async () => {
		const didDelete = await deleteFilesDirect(props.files, true);
		if (!didDelete) return;
		const deletedPaths = props.files
			.map((f) => {
				if (f.sd_path && "Physical" in f.sd_path) return f.sd_path.Physical.path;
				return null;
			})
			.filter((p): p is string => p !== null);
		await props.onDeleted(deletedPaths);
		dialog.state.open = false;
	}, [deleteFilesDirect, props, dialog]);

	return (
		<Dialog
			dialog={dialog}
			form={form}
			title={t("organize.deleteDialogTitle")}
			description={t("organize.deleteDialogDescription")}
			ctaLabel={t("organize.deleteDialogConfirm")}
			ctaDanger
			cancelBtn
			cancelLabel={t("organize.deleteDialogCancel")}
			loading={isPending}
			onSubmit={handleConfirm}
		/>
	);
}

export function openOrganizeDeleteDialog(args: {
	files: File[];
	onDeleted: (deletedPaths: string[]) => Promise<void> | void;
}) {
	return dialogManager.create((props) => (
		<OrganizeDeleteDialog {...props} files={args.files} onDeleted={args.onDeleted} />
	));
}
