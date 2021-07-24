import { AccountStatus, Notification } from "./json";
import { ContentManager, capitalizeFirstLetter } from './content';

class ActionListerner {
    specify_network: HTMLInputElement;
    specify_address: HTMLInputElement;
    btn_execute_action: HTMLButtonElement;
    manager: ContentManager;

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
                window.location.href = "?network="
                    + (document.getElementById("specify-network")! as HTMLInputElement).innerHTML.toLowerCase()
                    + "&address="
                    + (document.getElementById("specify-address")! as HTMLInputElement).value;
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
                console.log(msg);
                socket.send(msg);
            });

            socket.addEventListener("message", (event: Event) => {
                let msg = (event as MessageEvent);
                console.log(msg.data);
                this.parseAccountStatus(msg);
            });
        }
    }
    parseAccountStatus(msg: MessageEvent) {
        const parsed: AccountStatus = JSON.parse(msg.data);
        if (parsed.result_type == "ok") {
            this.manager.setLiveUpdateInfo();
            this.manager.processVerificationOverviewTable(parsed.message.state);
            this.manager.processUnsupportedOverview(parsed.message.state);
        } else if (parsed.result_type == "err") {
            // TODO: display error

            this.manager.wipeLiveUpdateInfo();
            this.btn_execute_action.innerHTML = `Go!`;
            this.btn_execute_action.disabled = false;
        } else {
            // Print unexpected error...
        }
    }
}

new ActionListerner();
