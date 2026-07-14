<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { Profile } from "$lib/Profile";

	import Copy from "phosphor-svelte/lib/Copy";
	import FloppyDisk from "phosphor-svelte/lib/FloppyDisk";
	import Pencil from "phosphor-svelte/lib/Pencil";
	import Trash from "phosphor-svelte/lib/Trash";
	import Popup from "./Popup.svelte";

	import { t } from "$lib/i18n";
	import { inspectedInstance } from "$lib/propertyInspector";

	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { message, open } from "@tauri-apps/plugin-dialog";

	let folders: { [name: string]: string[] } = {};
	let value: string;
	async function getProfiles(device: DeviceInfo) {
		let profiles: string[] = await invoke("get_profiles", { device: device.id });
		folders = {};
		for (const id of profiles) {
			let folder = id.includes("/") ? id.split("/")[0] : "";
			if (folders[folder]) folders[folder].push(id);
			else folders[folder] = [id];
		}
		profile = await invoke("get_selected_profile", { device: device.id });
		value = profile.id;
		oldValue = value;
	}

	export let device: DeviceInfo;
	getProfiles(device);

	export let profile: Profile;
	export async function setProfile(id: string) {
		if (!device || !id) return;
		if (value != id) {
			value = id;
			return;
		}
		await invoke("set_selected_profile", { device: device.id, id });
		profile = await invoke("get_selected_profile", { device: device.id });

		let folder = id.includes("/") ? id.split("/")[0] : "";
		if (folders[folder]) {
			if (!folders[folder].includes(id)) folders[folder].push(id);
		} else folders[folder] = [id];
		folders = folders;

		$inspectedInstance = null;
	}

	listen("rerender_images", async () => {
		try {
			profile = await invoke("get_selected_profile", { device: device.id });
		} catch {}
	});

	async function deleteProfile(id: string) {
		for (const devices of Object.values(applicationProfiles)) {
			if (devices[device.id] == id) {
				delete devices[device.id];
				applicationProfiles = applicationProfiles;
			}
		}
		await invoke("delete_profile", { device: device.id, profile: id });
		let folder = id.includes("/") ? id.split("/")[0] : "";
		folders[folder].splice(folders[folder].indexOf(id), 1);
		folders = folders;
	}

	let renamingProfile: string | null = null;
	let renameInput: HTMLInputElement;
	let newId: string = "";

	async function saveRenamedProfile(oldId: string) {
		if (!renameInput.checkValidity() || !newId) return;
		if (newId == oldId) {
			renamingProfile = null;
			return;
		}

		// Check if a profile with the new ID already exists
		const allProfiles = Object.values(folders).flat();
		if (allProfiles.includes(newId)) {
			message($t("profile_manager.rename.exists", { id: newId }), { title: $t("profile_manager.rename.failed"), buttons: { ok: $t("dialog.ok") } });
			return;
		}

		try {
			await invoke("rename_profile", { device: device.id, oldId, newId, retain: false });
		} catch (error: any) {
			message(error, { title: $t("profile_manager.rename.failed"), buttons: { ok: $t("dialog.ok") } });
			console.error(error);
		}

		// Update application profile mappings
		for (const devices of Object.values(applicationProfiles)) {
			if (devices[device.id] == oldId) devices[device.id] = newId;
		}
		applicationProfiles = applicationProfiles;

		// Update folders structure
		const oldFolder = oldId.includes("/") ? oldId.split("/")[0] : "";
		const newFolder = newId.includes("/") ? newId.split("/")[0] : "";

		// Remove from old folder
		if (folders[oldFolder]) {
			const index = folders[oldFolder].indexOf(oldId);
			if (index != -1) {
				folders[oldFolder].splice(index, 1);
				if (folders[oldFolder].length == 0 && oldFolder != "") delete folders[oldFolder];
			}
		}

		// Add to new folder
		if (folders[newFolder]) folders[newFolder].push(newId);
		else folders[newFolder] = [newId];

		folders = folders;
		renamingProfile = null;
	}
	$: if (renameInput) renameInput.focus();

	async function duplicateProfile(id: string) {
		let newId = id + $t("profile_manager.duplicate.suffix");

		// Check if a profile with the new ID already exists
		const allProfiles = Object.values(folders).flat();
		let counter = 1;
		while (allProfiles.includes(newId)) {
			counter++;
			newId = `${id}${$t("profile_manager.duplicate.suffix")} ${counter}`;
		}

		await invoke("rename_profile", { device: device.id, oldId: id, newId, retain: true });
		await getProfiles(device);
	}

	let oldValue: string;
	$: {
		if (value == "opendeck_edit_profiles") {
			if (oldValue) showPopup = true;
			value = oldValue;
		} else if (value && value != oldValue && (!profile || profile.id != value)) {
			setProfile(value);
			oldValue = value;
		}
	}

	let showPopup: boolean = false;
	let nameInput: HTMLInputElement;

	let applications: string[];
	let applicationProfiles: { [appName: string]: { [device: string]: string } };
	let openWindows: { title: string; class: string }[] = [];
	(async () => {
		applications = await invoke("get_applications");
		applicationProfiles = await invoke("get_application_profiles");
		try {
			openWindows = await invoke("get_open_windows");
		} catch (e) {
			console.error("Failed to query open windows", e);
		}
	})();
	listen("applications", ({ payload }: { payload: string[] }) => (applications = payload));

	$: {
		if (applicationProfiles) {
			applicationProfiles = Object.fromEntries(
				Object.entries(applicationProfiles).filter(([_, devices]) => Object.values(devices).filter((v) => v).length != 0),
			);
			invoke("set_application_profiles", { value: applicationProfiles });
		}
	}

	function getFilteredOpenWindows(wins: { title: string; class: string }[], activeApps: string[]) {
		const seen = new Set(activeApps || []);
		const filtered: { title: string; class: string }[] = [];
		for (const w of wins || []) {
			if (!seen.has(w.class)) {
				seen.add(w.class);
				filtered.push(w);
			}
		}
		return filtered.sort((a, b) => a.class.localeCompare(b.class));
	}

	function getAppForProfile(profileId: string): string {
		if (!applicationProfiles) return "";
		for (const [appName, devices] of Object.entries(applicationProfiles)) {
			if (devices[device.id] === profileId) {
				return appName;
			}
		}
		return "";
	}

	function setAppForProfile(profileId: string, appName: string) {
		if (!applicationProfiles) return;
		for (const app of Object.keys(applicationProfiles)) {
			if (applicationProfiles[app][device.id] === profileId) {
				delete applicationProfiles[app][device.id];
			}
		}
		if (appName) {
			applicationProfiles[appName] ||= {};
			applicationProfiles[appName][device.id] = profileId;
		}
		applicationProfiles = { ...applicationProfiles };
	}

	async function handleAppChange(profileId: string, val: string) {
		if (val === "opendeck_choose_app") {
			const path = await open({ multiple: false, directory: false });
			if (!path) {
				applicationProfiles = { ...applicationProfiles };
				return;
			}
			const appName = path.split(/[\/\\]/).at(-1) ?? path;
			if (!applications.includes(appName)) {
				applications = [...applications, appName];
				await invoke("add_application", { appName });
			}
			setAppForProfile(profileId, appName);
		} else {
			if (val && val !== "opendeck_default" && !applications.includes(val)) {
				applications = [...applications, val];
				await invoke("add_application", { appName: val });
			}
			setAppForProfile(profileId, val);
		}
	}

	let measure: HTMLSpanElement;
	let selectWidth = 0;
	$: if (value && measure) {
		measure.textContent = value.includes("/") ? value.split("/")[1] : value;
		selectWidth = measure.offsetWidth + 18;
	}
</script>

<div class="select-profile-wrapper">
	<span bind:this={measure} class="invisible fixed whitespace-pre pointer-events-none" aria-hidden="true"></span>
	<select bind:value style:width="{selectWidth}px" aria-label={$t("profile_manager.label")}>
		{#each Object.entries(folders).sort() as [id, profiles]}
			{#if id && profiles.length}
				<optgroup label={id}>
					{#each profiles.sort() as profile}
						<option value={profile}>{profile.split("/")[1]}</option>
					{/each}
				</optgroup>
			{:else}
				{#each profiles.sort() as profile}
					<option value={profile}>{profile}</option>
				{/each}
			{/if}
		{/each}
		<option value="opendeck_edit_profiles">{$t("profile_manager.edit")}</option>
	</select>
</div>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape") {
			if (renamingProfile) renamingProfile = null;
			else showPopup = false;
		}
	}}
/>

<Popup show={showPopup} label="{device.name} {$t('profile_manager.profiles')}">
	<button class="mr-1 float-right text-xl text-neutral-300" on:click={() => (showPopup = false)} aria-label={$t("settings.close")}>✕</button>
	<h2 class="text-xl font-semibold text-neutral-300">{device.name}</h2>

	<div class="flex flex-row mt-2 mb-1">
		<input
			bind:this={nameInput}
			pattern="[a-zA-Z0-9_ ]+(\/[a-zA-Z0-9_ ]+)?"
			class="grow p-2 text-neutral-300 invalid:text-red-400 bg-neutral-700 border-l border-y border-neutral-600 rounded-l-lg"
			placeholder={$t("profile_manager.create.placeholder")}
			aria-label={$t("profile_manager.create.label")}
		/>

		<button
			on:click={async () => {
				if (!nameInput.checkValidity() || !nameInput.value) return;
				await setProfile(nameInput.value);
				value = nameInput.value;
				nameInput.value = "";
				showPopup = false;
			}}
			class="px-4 text-neutral-300 bg-neutral-900 hover:bg-neutral-800 transition-colors border-r border-y border-neutral-600 rounded-r-lg"
		>
			{$t("profile_manager.create")}
		</button>
	</div>

	<div class="divide-y divide-neutral-500!">
		{#each Object.entries(folders).sort() as [id, profiles]}
			{#if id && profiles.length}
				<h4 class="py-2 font-bold text-lg text-neutral-300">{id}</h4>
			{/if}
			{#each profiles.sort() as profile}
				<div class="flex flex-row items-center py-2 space-x-2" class:ml-6={id} class:pl-2={id}>
					<input
						type="radio"
						bind:group={value}
						value={profile}
						disabled={renamingProfile == profile}
						id={`profile-${encodeURIComponent(profile)}`}
						aria-label={id ? profile.split("/")[1] : profile}
					/>
					{#if profile == renamingProfile}
						<!-- prettier-ignore -->
						<input
							bind:this={renameInput}
							bind:value={newId}
							pattern="[a-zA-Z0-9_ ]+(\/[a-zA-Z0-9_ ]+)?"
							class="grow px-2 py-1 text-neutral-300 invalid:text-red-400 bg-neutral-700 rounded"
							placeholder='Profile name or "folder/name"'
							on:keydown={(e) => {
								if (e.key === "Enter") saveRenamedProfile(profile);
							}}
						/>
						<button on:click={() => saveRenamedProfile(profile)} title={$t("profile_manager.save")} aria-label={$t("profile_manager.save")}>
							<FloppyDisk size="20" class="text-green-500" />
						</button>
					{:else}
						<label class="grow text-neutral-400" for={`profile-${encodeURIComponent(profile)}`}>{id ? profile.split("/")[1] : profile}</label>
						<div class="select-wrapper text-xs w-48">
							<select
								value={getAppForProfile(profile)}
								on:change={(e) => handleAppChange(profile, e.currentTarget.value)}
								aria-label={$t("profile_manager.smart_profile.label")}
								class="w-full"
							>
								<option value="">{$t("profile_manager.smart_profile.none")}</option>
								<option value="opendeck_default">{$t("profile_manager.default_profile")}</option>
								{#if applications && applications.length > 0}
									<optgroup label="Recently Active">
										{#each [...applications].sort() as appName}
											<option value={appName}>{appName}</option>
										{/each}
									</optgroup>
								{/if}
								{#if openWindows && openWindows.length > 0}
									<optgroup label="Currently Open">
										{#each getFilteredOpenWindows(openWindows, applications) as win}
											<option value={win.class}>{win.title ? `${win.title.slice(0, 30)}${win.title.length > 30 ? '...' : ''} (${win.class})` : win.class}</option>
										{/each}
									</optgroup>
								{/if}
								<option disabled>──────────</option>
								<option value="opendeck_choose_app">{$t("profile_manager.smart_profile.choose")}</option>
							</select>
						</div>
						<button on:click={() => duplicateProfile(profile)} title={$t("profile_manager.duplicate")} aria-label={$t("profile_manager.duplicate")}>
							<Copy size="20" class="text-neutral-400" />
						</button>
						{#if profile != value}
							<button on:click={() => (renamingProfile = newId = profile)} title={$t("profile_manager.rename")} aria-label={$t("profile_manager.rename")}>
								<Pencil size="20" class="text-neutral-400" />
							</button>
							<button on:click={() => deleteProfile(profile)} title={$t("profile_manager.delete")} aria-label={$t("profile_manager.delete")}>
								<Trash size="20" class="text-neutral-400" />
							</button>
						{/if}
					{/if}
				</div>
			{/each}
		{/each}
	</div>
</Popup>

