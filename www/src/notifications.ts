import { capitalizeFirstLetter } from "./content.js";
import { Notification, NotificationFieldContext, ManuallyVerified } from "./json";

export class NotificationHandler {
    notify_idx: number
    div_notifications: HTMLElement;

    constructor() {
        this.notify_idx = 0;

        this.div_notifications =
            document
                .getElementById("div-notifications")!;
    }

    processNotifications(notifications: Notification[]) {
        for (let notify of notifications) {
            try {
                const [message, color] = notificationTypeResolver(notify);
                this.displayNotification(message, color, false);
            } catch (error) {
                this.unexpectedError("pnntr#1");
            }
        }
    }
    displayNotification(message: string, color: string, show_final: boolean) {
        this.div_notifications.insertAdjacentHTML(
            "beforeend",
            `<div id="toast-${this.notify_idx}" class="toast show align-items-center ${color} border-0" role="alert" aria-live="assertive"
                    aria-atomic="true">
                    <div class="d-flex">
                        <div class="toast-body">
                            ${message}
                        </div>
                        <button id="toast-${this.notify_idx}-close-btn" type="button" class="btn-close btn-close-white me-2 m-auto" data-bs-dismiss="toast"
                            aria-label="Close"></button>
                    </div>
                </div>
            `
        );

        // Add handler for close button.
        let idx = this.notify_idx;
        document
            .getElementById(`toast-${idx}-close-btn`)!
            .addEventListener("click", (e: Event) => {
                let toast: HTMLElement = document
                    .getElementById(`toast-${idx}`)!;

                toast.classList.remove("show");
                toast.classList.add("hide");
            });

        // Cleanup old toast, limit to eight max.
        let max = 8;
        if (show_final) {
            max = 1;
        }

        let old = this.notify_idx - max;
        if (old >= 0) {
            let toast: HTMLElement | null = document
                .getElementById(`toast-${old}`);

            if (toast) {
                toast.classList.remove("show");
                toast.classList.add("hide");
            }
        }

        this.notify_idx += 1;
    }
    displayError(message: string) {
        this.displayNotification(message, "bg-danger text-light", false);
    }
    unexpectedError(id: string) {
        this.displayError(`Unexpected internal error, please contact admin. Code: ${id}`);
    }
}

function notificationTypeResolver(notification: Notification): [string, string] {
    switch (notification.type) {
        case "identity_inserted": {
            return [
                `The judgement request has been discovered by the registrar service.`,
                "bg-info text-dark"
            ]
        }
        case "identity_updated": {
            return [
                `On-chain identity information has been modified.`,
                "bg-info text-dark"
            ]
        }
        case "field_verified": {
            let data = notification.value as NotificationFieldContext;
            return [
                `${capitalizeFirstLetter(data.field.type)} account "${data.field.value}" is verified. Challenge is valid.`,
                "bg-success text-light",
            ]
        }
        case "field_verification_failed": {
            let data = notification.value as NotificationFieldContext;
            return [
                `${capitalizeFirstLetter(data.field.type)} account "${data.field.value}" failed to get verified. Invalid challenge.`,
                "bg-danger text-light"
            ]
        }
        case "second_field_verified": {
            let data = notification.value as NotificationFieldContext;
            return [
                `${capitalizeFirstLetter(data.field.type)} account "${data.field.value}" is fully verified. Additional challenge is valid.`,
                "bg-success text-light"
            ]
        }
        case "second_field_verification_failed": {
            let data = notification.value as NotificationFieldContext;
            return [
                `${capitalizeFirstLetter(data.field.type)} account "${data.field.value}" failed to get verified. The additional challenge is invalid.`,
                "bg-danger text-light"
            ]
        }
        case "awaiting_second_challenge": {
            let data = notification.value as NotificationFieldContext;
            return [
                `A second challenge was sent to ${capitalizeFirstLetter(data.field.type)} account "${data.field.value}". Please also check the spam folder.`,
                "bg-info text-dark"
            ]
        }
        case "identity_fully_verified": {
            return [
                `<strong>Verification process completed!</strong> Judgement will be issued in a couple of minutes.`,
                "bg-success text-light"
            ]
        }
        case "judgement_provided": {
            return [
                `Judgement has been submitted!`,
                "bg-success text-light"
            ]
        }
        case "manually_verified": {
            let data = notification.value as ManuallyVerified;
            return [
                `Manually verified ${capitalizeFirstLetter(data.field)}`,
                "bg-info text-light"
            ]
        }
        case "full_manual_verification": {
            return [
                `Manually verified the identity. Judgement will be issued in a couple of minutes.`,
                "bg-info text-light"
            ]
        }
        default: {
            throw new Error("unrecognized notification");
        }
    }
}