import { ValidMessage, AccountStatus, Notification, CheckDisplayNameResult, Violation } from "./json";
import { ContentManager, capitalizeFirstLetter, BadgeValid } from './content';
import { NotificationHandler } from "./notifications";

class ActionListerner {
    specify_network: HTMLInputElement;
    specify_address: HTMLInputElement;
    btn_execute_action: HTMLButtonElement;
    manager: ContentManager;
    notifications: NotificationHandler;

    constructor() {
        // Register relevant elements.
        this.btn_execute_action =
            document
                .getElementById("execute-action")! as HTMLButtonElement;

        this.specify_network =
            document
                .getElementById("specify-network")! as HTMLInputElement;

        this.specify_address =
            document
                .getElementById("specify-address")! as HTMLInputElement;

        this.manager = new ContentManager;
        this.notifications = new NotificationHandler;

        // Handler for choosing network, e.g. "Kusama" or "Polkadot".
        document
            .getElementById("network-options")!
            .addEventListener("click", (e: Event) => {
                this.specify_network
                    .innerText = (e.target as HTMLAnchorElement).innerText;
                this.btn_execute_action.innerHTML = `Go!`;
                this.btn_execute_action.disabled = false;
            });

        // Handler for choosing action, e.g. "Request Judgement".
        document
            .getElementById("action-options")!
            .addEventListener("click", (e: Event) => {
                document
                    .getElementById("specify-action")!
                    .innerText = (e.target as HTMLAnchorElement).innerText;
            });

        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_: Event) => {
                let action = document.getElementById("specify-action")!.innerHTML;
                if (action == "Request Judgement") {
                    window.location.href = "?network="
                        + (document.getElementById("specify-network")! as HTMLInputElement).innerHTML.toLowerCase()
                        + "&address="
                        + (document.getElementById("specify-address")! as HTMLInputElement).value;
                } else if (action == "Check Username") {
                    this.executeAction();
                }
            });

        this.specify_address
            .addEventListener("input", (_: Event) => {
                this.btn_execute_action.innerHTML = `Go!`;
                this.btn_execute_action.disabled = false;

                if (this.specify_address.value.startsWith("1")) {
                    this.specify_network.innerHTML = "Polkadot";
                } else {
                    this.specify_network.innerHTML = "Kusama";
                }
            });

        Array.from(document
            .getElementsByClassName("toast")!)
            .forEach(element => {
                element
                    .addEventListener("click", (_: Event) => {
                    });
            });

        let params = new URLSearchParams(window.location.search);
        let network = params.get("network");
        let address = params.get("address");

        if (network != null && address != null) {
            (document.getElementById("specify-network")! as HTMLInputElement).innerHTML = capitalizeFirstLetter(network);
            (document.getElementById("specify-address")! as HTMLInputElement).value = address;
            this.executeAction();
        }
    }
    executeAction() {
        this.btn_execute_action.disabled = true;

        const action = document.getElementById("specify-action")!.innerHTML;
        // TODO: Rename this, can be both address or display name.
        const address = this.specify_address.value;
        const network = this.specify_network.innerHTML.toLowerCase();

        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;

        if (action == "Request Judgement") {
            const socket = new WebSocket('ws://localhost:8001/api/account_status');

            socket.addEventListener("open", (_: Event) => {
                let msg = JSON.stringify({ address: address, chain: network });
                socket.send(msg);
            });

            socket.addEventListener("message", (event: Event) => {
                let msg = (event as MessageEvent);
                this.parseAccountStatus(msg);
            });
        } else if (action == "Check Username") {
            let display_name = address;
            (async () => {
                let body = JSON.stringify({
                    check: display_name,
                });

                let response = await fetch("http://localhost:8001/api/check_display_name",
                {
                    method: "POST",
                    headers: {
                        "Content-Type": "application/json",
                    },
                    body: body,
                });

                let result: AccountStatus = JSON.parse(await response.text());
                this.parseDisplayNameCheck(result, display_name);
            })();
        }
    }
    parseDisplayNameCheck(data: AccountStatus, display_name: string) {
        console.log(data);
        if (data.result_type == "ok") {
            let check: CheckDisplayNameResult = data.message;
            if (check.type == "ok") {
                this.manager.setDisplayNameVerification(display_name, BadgeValid);
            } else if (check.type = "violations") {
                let violations: Violation[] = check.value;
                this.manager.setDisplayNameViolation(display_name, violations);
            } else {
                // TODO
            }
        } else if (data.result_type == "err") {
            // TODO
        } else {
            // TODO
        }
    }
    parseAccountStatus(msg: MessageEvent) {
        const parsed: AccountStatus = JSON.parse(msg.data);
        if (parsed.result_type == "ok") {
            this.btn_execute_action.innerHTML = `
                <div class="spinner-grow spinner-grow-sm" role="status">
                    <span class="visually-hidden"></span>
                </div>
            `;

            let message: ValidMessage = parsed.message;
            this.manager.setLiveUpdateInfo();
            this.manager.processVerificationOverviewTable(message.state);
            this.manager.processUnsupportedOverview(message.state);
            this.notifications.processNotifications(message.notifications);
        } else if (parsed.result_type == "err") {
            let message: string = parsed.message;
            this.notifications.displayError(message);

            this.manager.wipeLiveUpdateInfo();
            this.btn_execute_action.innerHTML = `Go!`;
            this.btn_execute_action.disabled = false;
        } else {
            // TODO: Print unexpected error...
        }
    }
}

new ActionListerner();
