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

        // TODO: Rename this element.
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
                } else if (action == "Validate Display Name") {
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

        // Bind 'Enter' key to action button.
        this.specify_address
            .addEventListener("keyup", (event: Event) => {
                // Number 13 is the "Enter" key on the keyboard
                if ((event as KeyboardEvent).keyCode === 13) {
                    // Cancel the default action, if needed
                    event.preventDefault();
                    this.btn_execute_action.click();
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
            const socket = new WebSocket('wss://registrar-backend.web3.foundation/api/account_status');

            socket.addEventListener("open", (_: Event) => {
                let msg = JSON.stringify({ address: address, chain: network });
                socket.send(msg);
            });

            socket.addEventListener("message", (event: Event) => {
                let msg = (event as MessageEvent);
                this.parseAccountStatus(msg);
            });
        } else if (action == "Validate Display Name") {
            let display_name = address;
            (async () => {
                let body = JSON.stringify({
                    check: display_name,
                });

                let response = await fetch("https://registrar-backend.web3.foundation/api/check_display_name",
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
        if (data.type == "ok") {
            let check: CheckDisplayNameResult = data.message;
            if (check.type == "ok") {
                this.manager.setDisplayNameVerification(display_name, BadgeValid);
            } else if (check.type = "violations") {
                let violations: Violation[] = check.value;
                this.manager.setDisplayNameViolation(display_name, violations, false);
            } else {
                // TODO
            }
        } else if (data.type == "err") {
            // TODO
        } else {
            // TODO
        }

        this.btn_execute_action.innerHTML = `Go!`;
        this.btn_execute_action.disabled = false;

        this.manager.wipeLiveUpdateInfo();
        this.manager.wipeVerificationOverviewContent();
        this.manager.wipeEmailSecondChallengeContent();
        this.manager.wipeUnsupportedContent();
    }
    parseAccountStatus(msg: MessageEvent) {
        const parsed: AccountStatus = JSON.parse(msg.data);
        if (parsed.type == "ok") {
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

            // This notification should only be displayed if no other notifications are available.
            if (message.state.is_fully_verified && message.notifications.length == 0) {
                this.notifications.displayNotification("The identity has been fully verified!", "bg-success text-light")
            }
        } else if (parsed.type == "err") {
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
