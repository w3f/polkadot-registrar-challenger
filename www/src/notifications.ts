import { capitalizeFirstLetter } from "./content.js";
import { Notification, NotificationFieldContext } from "./json";

class NotificationHandler {
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
            let color;
            switch (notify.type) {
                case "success": {
                    color = "bg-success text-light";
                    break;
                }
                case "info": {
                    color = "bg-info text-light";
                    break;
                }
                case "warning": {
                    color = "bg-warning text-dark";
                    break;
                }
                case "error": {
                    color = "bg-danger text-light";
                    break;
                }
                default: {
                    // TODO
                }
            }

            this.div_notifications.innerHTML += `
                <div id="toast-${this.notify_idx}" class="toast show ${color}" role="alert" aria-live="assertive" aria-atomic="true" data-autohide="false">
                    <div class="toast-header">
                        <strong class="me-auto">Bootstrap</strong>
                        <small class="text-muted">just now</small>
                        <button type="button" class="btn-close" data-bs-dismiss="toast" aria-label="Close"></button>
                    </div>
                    <div class="toast-body">
                        See? Just like this.
                    </div>
                </div>
            `;
        }
	}
}

function notificationTypeResolver(notification: Notification): string {
    switch (notification.type) {
        case "field_verified": {
            let data: NotificationFieldContext = JSON.parse(notification.value);
            return `The ${capitalizeFirstLetter(data.field.type)} "${data.field.value}" as been verified.`
        }
        case "field_verification_failed": {

        }
        case "second_field_verified": {

        }
        case "second_field_verification": {

        }
        case "second_field_verification_failed": {

        }
        case "awaiting_second_challenge": {

        }
        case "identity_fully_verified": {

        }
        case "judgement_provided": {

        }
        default: {
            // TODO
        }
    }

    return ""
}