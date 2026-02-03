#!/usr/bin/env python3
"""
Auto-rigging and walk animation for humanoid characters.
Uses Blender's built-in tools to create skeleton, then generates walk cycle.

Usage:
    blender --background --python auto_animate.py -- input.glb output.glb
"""

import bpy
import sys
import math
from mathutils import Vector


def clear_scene():
    """Remove all objects from scene."""
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete()


def import_glb(filepath):
    """Import GLB file and return the mesh object."""
    bpy.ops.import_scene.gltf(filepath=filepath)

    mesh_obj = None
    for obj in bpy.context.scene.objects:
        if obj.type == 'MESH':
            mesh_obj = obj
            break

    if not mesh_obj:
        raise ValueError("No mesh found in GLB file")

    return mesh_obj


def get_mesh_bounds(mesh_obj):
    """Get world-space bounding box of mesh."""
    bbox = [mesh_obj.matrix_world @ Vector(corner) for corner in mesh_obj.bound_box]

    min_co = Vector((min(v.x for v in bbox), min(v.y for v in bbox), min(v.z for v in bbox)))
    max_co = Vector((max(v.x for v in bbox), max(v.y for v in bbox), max(v.z for v in bbox)))

    return {
        'min': min_co,
        'max': max_co,
        'center': (min_co + max_co) / 2,
        'size': max_co - min_co
    }


def create_simple_armature(mesh_obj, bounds):
    """
    Create a simple humanoid armature fitted to mesh bounds.
    """
    center_x = bounds['center'].x
    center_y = bounds['center'].y

    height = bounds['size'].z
    width = bounds['size'].x

    bottom = bounds['min'].z
    top = bounds['max'].z

    # Proportions (relative to height)
    hip_height = bottom + height * 0.45
    chest_height = bottom + height * 0.65
    shoulder_height = bottom + height * 0.75
    neck_height = bottom + height * 0.82
    head_height = top

    knee_height = bottom + height * 0.25
    foot_height = bottom

    hip_width = width * 0.15
    shoulder_width = width * 0.25

    # Create armature
    bpy.ops.object.armature_add(enter_editmode=True, location=(center_x, center_y, hip_height))
    armature_obj = bpy.context.active_object
    armature_obj.name = "Rig"
    armature = armature_obj.data
    armature.name = "RigData"

    bpy.ops.armature.select_all(action='SELECT')
    bpy.ops.armature.delete()

    def add_bone(name, head, tail, parent_name=None):
        bone = armature.edit_bones.new(name)
        bone.head = Vector(head)
        bone.tail = Vector(tail)
        if parent_name and parent_name in armature.edit_bones:
            bone.parent = armature.edit_bones[parent_name]
            bone.use_connect = (bone.head - bone.parent.tail).length < 0.01
        return bone

    # Spine chain
    add_bone("root", (center_x, center_y, hip_height), (center_x, center_y, hip_height + height * 0.05))
    add_bone("spine", (center_x, center_y, hip_height), (center_x, center_y, chest_height), "root")
    add_bone("chest", (center_x, center_y, chest_height), (center_x, center_y, shoulder_height), "spine")
    add_bone("neck", (center_x, center_y, shoulder_height), (center_x, center_y, neck_height), "chest")
    add_bone("head", (center_x, center_y, neck_height), (center_x, center_y, head_height), "neck")

    # Arms
    for side, sign in [("L", -1), ("R", 1)]:
        shoulder_x = center_x + sign * shoulder_width * 0.3
        hand_x = center_x + sign * shoulder_width * 1.5
        elbow_x = center_x + sign * shoulder_width * 0.9

        add_bone(f"shoulder.{side}",
                 (center_x, center_y, shoulder_height),
                 (shoulder_x, center_y, shoulder_height),
                 "chest")
        add_bone(f"upper_arm.{side}",
                 (shoulder_x, center_y, shoulder_height),
                 (elbow_x, center_y, shoulder_height - height * 0.05),
                 f"shoulder.{side}")
        add_bone(f"forearm.{side}",
                 (elbow_x, center_y, shoulder_height - height * 0.05),
                 (hand_x, center_y, shoulder_height - height * 0.1),
                 f"upper_arm.{side}")
        add_bone(f"hand.{side}",
                 (hand_x, center_y, shoulder_height - height * 0.1),
                 (hand_x + sign * width * 0.1, center_y, shoulder_height - height * 0.12),
                 f"forearm.{side}")

    # Legs
    for side, sign in [("L", -1), ("R", 1)]:
        hip_x = center_x + sign * hip_width

        add_bone(f"thigh.{side}",
                 (hip_x, center_y, hip_height),
                 (hip_x, center_y, knee_height),
                 "root")
        add_bone(f"shin.{side}",
                 (hip_x, center_y, knee_height),
                 (hip_x, center_y, foot_height + height * 0.05),
                 f"thigh.{side}")
        add_bone(f"foot.{side}",
                 (hip_x, center_y, foot_height + height * 0.05),
                 (hip_x, center_y - width * 0.1, foot_height),
                 f"shin.{side}")

    bpy.ops.object.mode_set(mode='OBJECT')
    return armature_obj


def parent_mesh_to_armature(mesh_obj, armature_obj):
    """Parent mesh to armature with automatic weights."""
    bpy.ops.object.select_all(action='DESELECT')
    mesh_obj.select_set(True)
    armature_obj.select_set(True)
    bpy.context.view_layer.objects.active = armature_obj
    bpy.ops.object.parent_set(type='ARMATURE_AUTO')


def create_walk_cycle(armature_obj, frame_count=24):
    """Create a simple walk cycle animation."""
    bpy.context.view_layer.objects.active = armature_obj
    bpy.ops.object.mode_set(mode='POSE')

    if not armature_obj.animation_data:
        armature_obj.animation_data_create()

    action = bpy.data.actions.new(name="WalkCycle")
    armature_obj.animation_data.action = action

    pose_bones = armature_obj.pose.bones

    leg_swing = math.radians(30)
    leg_lift = math.radians(15)
    arm_swing = math.radians(20)
    body_bob = 0.02

    def set_keyframe(bone_name, frame, rotation_euler=None, location=None):
        if bone_name not in pose_bones:
            return
        bone = pose_bones[bone_name]

        if rotation_euler:
            bone.rotation_mode = 'XYZ'
            bone.rotation_euler = rotation_euler
            bone.keyframe_insert(data_path="rotation_euler", frame=frame)

        if location:
            bone.location = location
            bone.keyframe_insert(data_path="location", frame=frame)

    key_frames = [1, 7, 13, 19, 24]

    for i, frame in enumerate(key_frames):
        phase = i % 4

        if phase in [1, 3]:
            set_keyframe("root", frame, location=(0, 0, body_bob))
        else:
            set_keyframe("root", frame, location=(0, 0, -body_bob))

        if phase == 0:
            set_keyframe("thigh.L", frame, rotation_euler=(leg_swing, 0, 0))
            set_keyframe("thigh.R", frame, rotation_euler=(-leg_swing, 0, 0))
            set_keyframe("shin.L", frame, rotation_euler=(0, 0, 0))
            set_keyframe("shin.R", frame, rotation_euler=(leg_lift, 0, 0))
        elif phase == 1:
            set_keyframe("thigh.L", frame, rotation_euler=(0, 0, 0))
            set_keyframe("thigh.R", frame, rotation_euler=(0, 0, 0))
            set_keyframe("shin.L", frame, rotation_euler=(leg_lift * 2, 0, 0))
            set_keyframe("shin.R", frame, rotation_euler=(0, 0, 0))
        elif phase == 2:
            set_keyframe("thigh.L", frame, rotation_euler=(-leg_swing, 0, 0))
            set_keyframe("thigh.R", frame, rotation_euler=(leg_swing, 0, 0))
            set_keyframe("shin.L", frame, rotation_euler=(leg_lift, 0, 0))
            set_keyframe("shin.R", frame, rotation_euler=(0, 0, 0))
        elif phase == 3:
            set_keyframe("thigh.L", frame, rotation_euler=(0, 0, 0))
            set_keyframe("thigh.R", frame, rotation_euler=(0, 0, 0))
            set_keyframe("shin.L", frame, rotation_euler=(0, 0, 0))
            set_keyframe("shin.R", frame, rotation_euler=(leg_lift * 2, 0, 0))

        if phase in [0, 4]:
            set_keyframe("upper_arm.L", frame, rotation_euler=(arm_swing, 0, 0))
            set_keyframe("upper_arm.R", frame, rotation_euler=(-arm_swing, 0, 0))
        elif phase == 2:
            set_keyframe("upper_arm.L", frame, rotation_euler=(-arm_swing, 0, 0))
            set_keyframe("upper_arm.R", frame, rotation_euler=(arm_swing, 0, 0))
        else:
            set_keyframe("upper_arm.L", frame, rotation_euler=(0, 0, 0))
            set_keyframe("upper_arm.R", frame, rotation_euler=(0, 0, 0))

    for fcurve in action.fcurves:
        for keyframe in fcurve.keyframe_points:
            keyframe.interpolation = 'BEZIER'

    bpy.context.scene.frame_start = 1
    bpy.context.scene.frame_end = frame_count

    bpy.ops.object.mode_set(mode='OBJECT')


def export_glb(filepath):
    """Export scene as animated GLB."""
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format='GLB',
        export_animations=True,
        export_frame_range=True,
        export_nla_strips=False,
        export_current_frame=False
    )


def main():
    argv = sys.argv
    if "--" in argv:
        argv = argv[argv.index("--") + 1:]
    else:
        argv = []

    if len(argv) < 2:
        print("Usage: blender --background --python auto_animate.py -- input.glb output.glb")
        sys.exit(1)

    input_path = argv[0]
    output_path = argv[1]

    print(f"Input: {input_path}")
    print(f"Output: {output_path}")

    print("Clearing scene...")
    clear_scene()

    print("Importing model...")
    mesh_obj = import_glb(input_path)

    print("Analyzing mesh bounds...")
    bounds = get_mesh_bounds(mesh_obj)
    print(f"  Size: {bounds['size'].x:.2f} x {bounds['size'].y:.2f} x {bounds['size'].z:.2f}")

    print("Creating armature...")
    armature_obj = create_simple_armature(mesh_obj, bounds)

    print("Parenting with automatic weights...")
    parent_mesh_to_armature(mesh_obj, armature_obj)

    print("Creating walk cycle animation...")
    create_walk_cycle(armature_obj)

    print("Exporting...")
    export_glb(output_path)

    print(f"Done! Animated model saved to: {output_path}")


if __name__ == "__main__":
    main()
