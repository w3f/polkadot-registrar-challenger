import { StateNotification, GenericMessage, Notification, CheckDisplayNameResult, Violation } from "./json";
import { ContentManager, capitalizeFirstLetter, BadgeValid } from './content';
import { NotificationHandler } from "./notifications";

interface Config {
    http_url: string;
    ws_url: string;
}

const config: Config = require("../config.json");

class ActionListerner {
    specify_network: HTMLInputElement;
    specify_action: HTMLInputElement;
    search_bar: HTMLInputElement;
    btn_execute_action: HTMLButtonElement;
    manager: ContentManager;
    notifications: NotificationHandler;

    constructor() {

        // Register relevant elements.
        this.btn_execute_action =
            document
                .getElementById("execute-action")! as HTMLButtonElement;

        this.specify_action =
            document
                .getElementById("specify-action")! as HTMLInputElement;

        this.specify_network =
            document
                .getElementById("specify-network")! as HTMLInputElement;

        this.search_bar =
            document
                .getElementById("search-bar")! as HTMLInputElement;

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

        // Handler for choosing action, e.g. "Check Judgement".
        document
            .getElementById("action-options")!
            .addEventListener("click", (e: Event) => {
                let target = (e.target as HTMLAnchorElement).innerText;
                if (target == "Check Judgement") {
                    this.search_bar.placeholder = "Account address..."
                    this.specify_action.innerText = target;
                } else if (target == "Validate Display Name") {
                    this.search_bar.placeholder = "Display Name..."
                    this.specify_action.innerText = target;
                }
            });

        // Handler for executing action and communicating with the backend API.
        this.btn_execute_action
            .addEventListener("click", (_: Event) => {
                let action = document.getElementById("specify-action")!.innerHTML;
                if (action == "Check Judgement") {
                    window.location.href = "?network="
                        + (document.getElementById("specify-network")! as HTMLInputElement).innerHTML.toLowerCase()
                        + "&address="
                        + (document.getElementById("search-bar")! as HTMLInputElement).value;
                } else if (action == "Validate Display Name") {
                    this.executeAction();
                }
            });

        this.search_bar
            .addEventListener("input", (_: Event) => {
                this.btn_execute_action.innerHTML = `Go!`;
                this.btn_execute_action.disabled = false;

                if (this.search_bar.value.startsWith("1")) {
                    this.specify_network.innerHTML = "Polkadot";
                } else {
                    this.specify_network.innerHTML = "Kusama";
                }
            });

        // Bind 'Enter' key to action button.
        this.search_bar
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
            (document.getElementById("search-bar")! as HTMLInputElement).value = address;
            this.executeAction();
        }
    }
    executeAction() {
        this.btn_execute_action.disabled = true;

        const action = document.getElementById("specify-action")!.innerHTML;
        const user_input = this.search_bar.value;
        const network = this.specify_network.innerHTML.toLowerCase();

        this.btn_execute_action
            .innerHTML = `
                <span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span>
                <span class="visually-hidden"></span>
            `;

        if (action == "Check Judgement") {
            const socket = new WebSocket(config.ws_url);

            window.setInterval(() => {
                socket.send("heartbeat");
            }, 30000);

            socket.onopen = () => {
                let msg = JSON.stringify({ address: user_input, chain: network });
                socket.send(msg);
            };

            socket.onmessage = (event: Event) => {
                let msg = (event as MessageEvent);
                this.parseAccountStatus(msg);
            };
        } else if (action == "Validate Display Name") {
            let display_name = user_input;

            (async () => {
                let body = JSON.stringify({
                    check: display_name,
                    chain: network,
                });

                let response = await fetch(config.http_url,
                    {
                        method: "POST",
                        headers: {
                            "Content-Type": "application/json",
                        },
                        body: body,
                    });

                let result: GenericMessage = JSON.parse(await response.text());
                this.parseDisplayNameCheck(result, display_name);
            })();
        }
    }
    parseDisplayNameCheck(data: GenericMessage, display_name: string) {
        document.getElementById("introduction")!.innerHTML = "";
        if (data.type == "ok") {
            let check: CheckDisplayNameResult = data.message;
            if (check.type == "ok") {
                this.manager.setDisplayNameVerification(display_name, BadgeValid);
            } else if (check.type = "violations") {
                let violations: Violation[] = check.value;
                this.manager.setDisplayNameViolation(display_name, violations, false);
            } else {
                // Should never occur.
                this.notifications.unexpectedError("pdnc#1")
            }
        } else if (data.type == "err") {
            // Should never occur.
            this.notifications.unexpectedError("pdnc#2")
        } else {
            // Should never occur.
            this.notifications.unexpectedError("pdnc#3")
        }

        this.btn_execute_action.innerHTML = `Go!`;
        this.btn_execute_action.disabled = false;

        this.manager.wipeLiveUpdateInfo();
        this.manager.wipeVerificationOverviewContent();
        this.manager.wipeEmailSecondChallengeContent();
        this.manager.wipeUnsupportedContent();
    }
    parseAccountStatus(msg: MessageEvent) {
        const parsed: GenericMessage = JSON.parse(msg.data);
        if (parsed.type == "ok") {
            document.getElementById("introduction")!.innerHTML = "";
            this.btn_execute_action.innerHTML = `
                <div class="spinner-grow spinner-grow-sm" role="status">
                    <span class="visually-hidden"></span>
                </div>
            `;

            let message: StateNotification = parsed.message;
            this.manager.setLiveUpdateInfo();
            this.manager.processVerificationOverviewTable(message.state);
            this.manager.processUnsupportedOverview(message.state);
            this.notifications.processNotifications(message.notifications);

            // This notification should only be displayed if no other notifications are available.
            if (message.state.is_fully_verified && message.notifications.length == 0) {
                this.notifications.displayNotification("The identity has been fully verified!", "bg-success text-light", true)
            }
        } else if (parsed.type == "err") {
            let message: string = parsed.message;
            this.notifications.displayError(message);

            this.manager.wipeLiveUpdateInfo();
            this.btn_execute_action.innerHTML = `Go!`;
            this.btn_execute_action.disabled = false;
        } else {
            // Should never occur.
            this.notifications.unexpectedError("pas#1")
        }
    }
}

new ActionListerner();
